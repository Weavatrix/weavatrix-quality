//! Strict `OpenSpec` change-delta reader.
//!
//! Parses `openspec/changes/<change>/specs/**/spec.md` and preserves
//! file/line provenance. Unknown headers and malformed nesting fail closed.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;
use wvq_domain::{ChangeId, RequirementId, ScenarioId};

/// One `OpenSpec` change folder, with every capability delta that was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSpecChange {
    /// Change folder name (`sankey-others`).
    pub id: ChangeId,
    /// Capability deltas in deterministic path order.
    pub capabilities: Vec<CapabilityDelta>,
}

/// Deltas for one capability (`openspec/changes/<change>/specs/<capability>/spec.md`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityDelta {
    /// Capability path under `specs/`, using `/` separators (`sankey`).
    pub capability: String,
    /// Repo-relative path of the delta file.
    pub source: PathBuf,
    /// Requirement operations in source order.
    pub operations: Vec<RequirementOp>,
}

/// One ADDED / MODIFIED / REMOVED / RENAMED operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequirementOp {
    /// New requirement.
    Added(RequirementDelta),
    /// Replacement text for an existing requirement.
    Modified(RequirementDelta),
    /// Requirement scheduled for removal.
    Removed(RequirementDelta),
    /// Identity rename. Content is not restated.
    Renamed {
        /// Previous requirement title.
        from: String,
        /// New requirement title.
        to: String,
        /// Location of the `FROM` line.
        location: SourceLocation,
    },
}

/// A requirement as written in a delta file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementDelta {
    /// Stable identity: `<capability-dots>.<slugged-title>`.
    pub id: RequirementId,
    /// Title from `### Requirement: …`.
    pub name: String,
    /// Normative body (not including scenario blocks).
    pub text: String,
    /// Scenarios in source order.
    pub scenarios: Vec<ScenarioDelta>,
    /// Header location.
    pub location: SourceLocation,
}

/// One `#### Scenario` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioDelta {
    /// Stable identity: slugged scenario title.
    pub id: ScenarioId,
    /// Title from `#### Scenario: …`.
    pub name: String,
    /// GIVEN / WHEN / THEN / AND clauses in source order.
    pub clauses: Vec<Clause>,
    /// Header location.
    pub location: SourceLocation,
}

/// One GIVEN / WHEN / THEN / AND line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clause {
    /// Clause kind.
    pub kind: ClauseKind,
    /// Text after the keyword.
    pub text: String,
    /// 1-based source line.
    pub line: u32,
}

/// BDD keyword.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClauseKind {
    /// Precondition.
    Given,
    /// Action.
    When,
    /// Expected result.
    Then,
    /// Continuation of the previous kind.
    And,
}

/// File + 1-based line of a parsed construct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    /// Path relative to the repository root passed to [`read_change`].
    pub file: PathBuf,
    /// 1-based line number.
    pub line: u32,
}

/// Why an `OpenSpec` change could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SpecError {
    /// Change identity is empty or otherwise illegal.
    #[error("invalid change identity: {0}")]
    InvalidChangeId(String),
    /// `openspec/changes/<change>` is missing.
    #[error("change not found: {0}")]
    ChangeNotFound(String),
    /// Change folder has no `specs/**/spec.md` deltas.
    #[error("change {0} has no OpenSpec delta specs")]
    NoDeltaSpecs(String),
    /// Filesystem failure while walking or reading.
    #[error("io error at {path}: {message}")]
    Io {
        /// Path that failed.
        path: String,
        /// Underlying message.
        message: String,
    },
    /// Header or nesting that `OpenSpec` / WVQ refuse to guess about.
    #[error("{file}:{line}: {message}")]
    InvalidSyntax {
        /// Repo-relative file.
        file: String,
        /// 1-based line.
        line: u32,
        /// Why parsing stopped.
        message: String,
    },
}

/// Read one change folder under `root/openspec/changes/<change>`.
///
/// # Errors
///
/// Returns [`SpecError`] when the change is missing, has no delta specs, or
/// any delta file has unknown headers or malformed nesting.
pub fn read_change(root: &Path, change: &str) -> Result<OpenSpecChange, SpecError> {
    let id = ChangeId::new(change).map_err(|err| SpecError::InvalidChangeId(err.to_string()))?;
    let change_dir = root.join("openspec").join("changes").join(change);
    if !change_dir.is_dir() {
        return Err(SpecError::ChangeNotFound(change.to_owned()));
    }

    let specs_dir = change_dir.join("specs");
    let mut spec_files = collect_spec_files(&specs_dir)?;
    spec_files.sort();
    if spec_files.is_empty() {
        return Err(SpecError::NoDeltaSpecs(change.to_owned()));
    }

    let mut capabilities = Vec::with_capacity(spec_files.len());
    for abs in spec_files {
        let rel = abs
            .strip_prefix(root)
            .unwrap_or(abs.as_path())
            .to_path_buf();
        let capability = capability_from_spec_path(&abs, &specs_dir)?;
        let source = fs::read_to_string(&abs).map_err(|err| SpecError::Io {
            path: rel.display().to_string(),
            message: err.to_string(),
        })?;
        let operations = parse_delta(&rel, &capability, &source)?;
        capabilities.push(CapabilityDelta {
            capability,
            source: rel,
            operations,
        });
    }

    Ok(OpenSpecChange { id, capabilities })
}

fn collect_spec_files(specs_dir: &Path) -> Result<Vec<PathBuf>, SpecError> {
    if !specs_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    collect_spec_files_rec(specs_dir, &mut out)?;
    Ok(out)
}

fn collect_spec_files_rec(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), SpecError> {
    let entries = fs::read_dir(dir).map_err(|err| SpecError::Io {
        path: dir.display().to_string(),
        message: err.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|err| SpecError::Io {
            path: dir.display().to_string(),
            message: err.to_string(),
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|err| SpecError::Io {
            path: path.display().to_string(),
            message: err.to_string(),
        })?;
        if file_type.is_dir() {
            collect_spec_files_rec(&path, out)?;
        } else if file_type.is_file() && path.file_name().is_some_and(|name| name == "spec.md") {
            out.push(path);
        }
    }
    Ok(())
}

fn capability_from_spec_path(spec_file: &Path, specs_dir: &Path) -> Result<String, SpecError> {
    let parent = spec_file.parent().ok_or_else(|| SpecError::Io {
        path: spec_file.display().to_string(),
        message: "spec.md has no parent directory".to_owned(),
    })?;
    let rel = parent
        .strip_prefix(specs_dir)
        .map_err(|_| SpecError::Io {
            path: spec_file.display().to_string(),
            message: "spec.md is not under specs/".to_owned(),
        })?;
    if rel.as_os_str().is_empty() {
        return Err(SpecError::Io {
            path: spec_file.display().to_string(),
            message: "spec.md must live under a capability directory".to_owned(),
        });
    }
    Ok(rel.to_string_lossy().replace('\\', "/"))
}

#[derive(Clone, Copy)]
enum Section {
    Added,
    Modified,
    Removed,
    Renamed,
    Ignore,
}

fn parse_delta(
    file: &Path,
    capability: &str,
    source: &str,
) -> Result<Vec<RequirementOp>, SpecError> {
    let mut parser = DeltaParser {
        file,
        ops: Vec::new(),
        section: None,
        current: None,
        pending_from: None,
    };
    for (idx, raw_line) in source.lines().enumerate() {
        let line_no = u32::try_from(idx + 1).unwrap_or(u32::MAX);
        parser.push_line(line_no, raw_line.trim())?;
    }
    parser.finish(source, capability)
}

struct DeltaParser<'a> {
    file: &'a Path,
    ops: Vec<RequirementOp>,
    section: Option<Section>,
    current: Option<OpenRequirement>,
    pending_from: Option<(String, u32)>,
}

impl DeltaParser<'_> {
    fn push_line(&mut self, line_no: u32, trimmed: &str) -> Result<(), SpecError> {
        if trimmed.is_empty() {
            return Ok(());
        }
        if let Some(title) = heading(trimmed, 2) {
            return self.begin_section(line_no, title);
        }
        if heading(trimmed, 1).is_some() {
            return Ok(());
        }
        if let Some(title) = heading(trimmed, 3) {
            return self.begin_requirement(line_no, title);
        }
        if let Some(title) = heading(trimmed, 4) {
            return self.begin_scenario(line_no, title);
        }
        if trimmed.starts_with('#') {
            return syntax(self.file, line_no, "unsupported heading level");
        }
        self.push_body(line_no, trimmed)
    }

    fn begin_section(&mut self, line_no: u32, title: &str) -> Result<(), SpecError> {
        flush_requirement(&mut self.ops, &mut self.current, self.file)?;
        if self.pending_from.is_some() {
            return syntax(
                self.file,
                line_no,
                "RENAMED FROM without a matching TO before the next section",
            );
        }
        self.section = Some(parse_section(self.file, line_no, title)?);
        Ok(())
    }

    fn begin_requirement(&mut self, line_no: u32, title: &str) -> Result<(), SpecError> {
        let name = requirement_title(self.file, line_no, title)?;
        match self.section {
            None | Some(Section::Ignore) => syntax(
                self.file,
                line_no,
                "requirement is not under an ADDED/MODIFIED/REMOVED/RENAMED section",
            ),
            Some(Section::Renamed) => syntax(
                self.file,
                line_no,
                "RENAMED requirements must use FROM/TO lines, not ### Requirement headers",
            ),
            Some(kind) => {
                flush_requirement(&mut self.ops, &mut self.current, self.file)?;
                self.current = Some(OpenRequirement {
                    kind,
                    name,
                    text_lines: Vec::new(),
                    scenarios: Vec::new(),
                    open_scenario: None,
                    location: SourceLocation {
                        file: self.file.to_path_buf(),
                        line: line_no,
                    },
                });
                Ok(())
            }
        }
    }

    fn begin_scenario(&mut self, line_no: u32, title: &str) -> Result<(), SpecError> {
        let name = scenario_title(self.file, line_no, title)?;
        let Some(req) = self.current.as_mut() else {
            return syntax(
                self.file,
                line_no,
                "scenario is not nested under a requirement",
            );
        };
        if matches!(req.kind, Section::Renamed) {
            return syntax(
                self.file,
                line_no,
                "RENAMED sections cannot contain scenarios",
            );
        }
        req.flush_scenario();
        req.open_scenario = Some(ScenarioDelta {
            id: slug_scenario(self.file, line_no, &name)?,
            name,
            clauses: Vec::new(),
            location: SourceLocation {
                file: self.file.to_path_buf(),
                line: line_no,
            },
        });
        Ok(())
    }

    fn push_body(&mut self, line_no: u32, trimmed: &str) -> Result<(), SpecError> {
        if matches!(self.section, Some(Section::Ignore)) {
            return Ok(());
        }
        if matches!(self.section, Some(Section::Renamed)) {
            return parse_rename_line(
                self.file,
                line_no,
                trimmed,
                &mut self.pending_from,
                &mut self.ops,
            );
        }
        let Some(req) = self.current.as_mut() else {
            return syntax(
                self.file,
                line_no,
                "text is not attached to a requirement or recognised section",
            );
        };
        if let Some(clause) = parse_clause(trimmed, line_no) {
            let Some(scenario) = req.open_scenario.as_mut() else {
                return syntax(
                    self.file,
                    line_no,
                    "GIVEN/WHEN/THEN clause is not under a scenario",
                );
            };
            if clause.kind == ClauseKind::And && scenario.clauses.is_empty() {
                return syntax(self.file, line_no, "AND has no preceding GIVEN/WHEN/THEN");
            }
            scenario.clauses.push(clause);
            return Ok(());
        }
        if req.open_scenario.is_some() {
            return syntax(
                self.file,
                line_no,
                "non-clause text is not allowed inside a scenario",
            );
        }
        req.text_lines.push(trimmed.to_owned());
        Ok(())
    }

    fn finish(mut self, source: &str, capability: &str) -> Result<Vec<RequirementOp>, SpecError> {
        if self.pending_from.is_some() {
            return syntax(
                self.file,
                u32::try_from(source.lines().count()).unwrap_or(u32::MAX),
                "RENAMED FROM without a matching TO",
            );
        }
        flush_requirement(&mut self.ops, &mut self.current, self.file)?;
        attach_ids(capability, &mut self.ops)?;
        Ok(self.ops)
    }
}

struct OpenRequirement {
    kind: Section,
    name: String,
    text_lines: Vec<String>,
    scenarios: Vec<ScenarioDelta>,
    open_scenario: Option<ScenarioDelta>,
    location: SourceLocation,
}

impl OpenRequirement {
    fn flush_scenario(&mut self) {
        if let Some(scenario) = self.open_scenario.take() {
            self.scenarios.push(scenario);
        }
    }
}

fn flush_requirement(
    ops: &mut Vec<RequirementOp>,
    current: &mut Option<OpenRequirement>,
    file: &Path,
) -> Result<(), SpecError> {
    let Some(mut req) = current.take() else {
        return Ok(());
    };
    req.flush_scenario();
    if matches!(req.kind, Section::Added | Section::Modified) && req.scenarios.is_empty() {
        return syntax(
            file,
            req.location.line,
            "ADDED/MODIFIED requirement must include at least one scenario",
        );
    }
    let delta = RequirementDelta {
        id: RequirementId::new("pending.placeholder")
            .expect("placeholder id is non-empty"),
        name: req.name,
        text: req.text_lines.join("\n"),
        scenarios: req.scenarios,
        location: req.location,
    };
    ops.push(match req.kind {
        Section::Added => RequirementOp::Added(delta),
        Section::Modified => RequirementOp::Modified(delta),
        Section::Removed => RequirementOp::Removed(delta),
        Section::Renamed | Section::Ignore => {
            return syntax(
                file,
                delta.location.line,
                "internal error: requirement flushed from a non-requirement section",
            );
        }
    });
    Ok(())
}

fn attach_ids(capability: &str, ops: &mut [RequirementOp]) -> Result<(), SpecError> {
    let prefix = capability.replace('/', ".");
    for op in ops {
        match op {
            RequirementOp::Added(delta)
            | RequirementOp::Modified(delta)
            | RequirementOp::Removed(delta) => {
                let slug = slug_title(&delta.name);
                let id = format!("{prefix}.{slug}");
                delta.id = RequirementId::new(&id).map_err(|_| SpecError::InvalidSyntax {
                    file: delta.location.file.display().to_string(),
                    line: delta.location.line,
                    message: format!("cannot form requirement id from `{id}`"),
                })?;
            }
            RequirementOp::Renamed { .. } => {}
        }
    }
    Ok(())
}

fn parse_section(file: &Path, line: u32, title: &str) -> Result<Section, SpecError> {
    match title {
        "ADDED Requirements" => Ok(Section::Added),
        "MODIFIED Requirements" => Ok(Section::Modified),
        "REMOVED Requirements" => Ok(Section::Removed),
        "RENAMED Requirements" => Ok(Section::Renamed),
        "Purpose" => Ok(Section::Ignore),
        other => syntax(
            file,
            line,
            &format!("unknown section `## {other}`"),
        ),
    }
}

fn heading(trimmed: &str, level: usize) -> Option<&str> {
    let prefix = match level {
        1 => "# ",
        2 => "## ",
        3 => "### ",
        4 => "#### ",
        _ => return None,
    };
    trimmed.strip_prefix(prefix).map(str::trim)
}

fn requirement_title(file: &Path, line: u32, title: &str) -> Result<String, SpecError> {
    let Some(name) = title.strip_prefix("Requirement:") else {
        return syntax(
            file,
            line,
            "level-3 heading must be `### Requirement: <name>`",
        );
    };
    let name = name.trim();
    if name.is_empty() {
        return syntax(file, line, "requirement title is empty");
    }
    Ok(name.to_owned())
}

fn scenario_title(file: &Path, line: u32, title: &str) -> Result<String, SpecError> {
    let Some(name) = title.strip_prefix("Scenario:") else {
        return syntax(file, line, "level-4 heading must be `#### Scenario: <name>`");
    };
    let name = name.trim();
    if name.is_empty() {
        return syntax(file, line, "scenario title is empty");
    }
    Ok(name.to_owned())
}

fn parse_clause(trimmed: &str, line: u32) -> Option<Clause> {
    let item = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))?;
    let item = item.trim();
    let (keyword, rest) = item.split_once(char::is_whitespace).unwrap_or((item, ""));
    let keyword = keyword.trim_matches('*');
    let kind = match keyword.to_ascii_uppercase().as_str() {
        "GIVEN" => ClauseKind::Given,
        "WHEN" => ClauseKind::When,
        "THEN" => ClauseKind::Then,
        "AND" => ClauseKind::And,
        _ => return None,
    };
    Some(Clause {
        kind,
        text: rest.trim().to_owned(),
        line,
    })
}

fn parse_rename_line(
    file: &Path,
    line: u32,
    trimmed: &str,
    pending_from: &mut Option<(String, u32)>,
    ops: &mut Vec<RequirementOp>,
) -> Result<(), SpecError> {
    let item = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
        .ok_or_else(|| invalid(file, line, "RENAMED lines must be `- FROM:` / `- TO:` bullets"))?;
    let (label, value) = item
        .split_once(':')
        .ok_or_else(|| invalid(file, line, "RENAMED bullet must be `FROM:` or `TO:`"))?;
    let value = strip_requirement_ref(value.trim());
    if value.is_empty() {
        return syntax(file, line, "RENAMED name is empty");
    }
    match label.trim().to_ascii_uppercase().as_str() {
        "FROM" => {
            if pending_from.is_some() {
                return syntax(file, line, "RENAMED FROM without a matching TO");
            }
            *pending_from = Some((value, line));
            Ok(())
        }
        "TO" => {
            let Some((from, from_line)) = pending_from.take() else {
                return syntax(file, line, "RENAMED TO without a preceding FROM");
            };
            ops.push(RequirementOp::Renamed {
                from,
                to: value,
                location: SourceLocation {
                    file: file.to_path_buf(),
                    line: from_line,
                },
            });
            Ok(())
        }
        _ => syntax(file, line, "RENAMED bullet must be `FROM:` or `TO:`"),
    }
}

fn strip_requirement_ref(raw: &str) -> String {
    let unquoted = raw.trim_matches('`').trim();
    unquoted
        .strip_prefix("### Requirement:")
        .or_else(|| unquoted.strip_prefix("Requirement:"))
        .unwrap_or(unquoted)
        .trim()
        .to_owned()
}

fn slug_title(name: &str) -> String {
    let mut out = String::new();
    let mut pending_hyphen = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_hyphen && !out.is_empty() {
                out.push('-');
            }
            pending_hyphen = false;
            out.push(ch.to_ascii_lowercase());
        } else if !out.is_empty() {
            pending_hyphen = true;
        }
    }
    out
}

fn slug_scenario(file: &Path, line: u32, name: &str) -> Result<ScenarioId, SpecError> {
    let slug = slug_title(name);
    ScenarioId::new(&slug).map_err(|_| invalid(file, line, &format!("cannot form scenario id from `{name}`")))
}

fn syntax<T>(file: &Path, line: u32, message: &str) -> Result<T, SpecError> {
    Err(invalid(file, line, message))
}

fn invalid(file: &Path, line: u32, message: &str) -> SpecError {
    SpecError::InvalidSyntax {
        file: file.display().to_string(),
        line,
        message: message.to_owned(),
    }
}
