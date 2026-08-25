//! Changed-region mutation. Survived mutants are proof weakness, not coverage.

use std::collections::BTreeSet;

use sha2::{Digest, Sha256};
use thiserror::Error;

/// Ecosystem for a mutant. v1: TS/JS and Go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutantEcosystem {
    /// TypeScript / JavaScript.
    TsJs,
    /// Go.
    Go,
}

/// TS/JS operators from spec §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TsJsOperator {
    /// `>` ↔ `>=`
    CmpGtGe,
    /// `<` ↔ `<=`
    CmpLtLe,
    /// `===` ↔ `!==`
    EqNeq,
    /// `true` ↔ `false`
    BoolFlip,
    /// `&&` ↔ `||`
    AndOr,
    /// `+1` ↔ `-1`
    OffByOne,
    /// remove branch
    RemoveBranch,
    /// remove sort
    RemoveSort,
    /// wrong permission
    WrongPermission,
    /// omit callback
    OmitCallback,
    /// omit error propagation
    OmitError,
    /// wrong collection boundary
    CollectionBoundary,
}

/// Safe Go operators from spec §21.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoOperator {
    /// `err != nil` ↔ `err == nil`
    ErrNilFlip,
    /// boundary flip
    BoundaryFlip,
    /// return nil/zero
    ReturnZero,
    /// skip branch
    SkipBranch,
    /// ignore context
    IgnoreContext,
    /// invert boolean
    InvertBool,
}

/// One operator application on a changed region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutant {
    /// Mutant id.
    pub id: String,
    /// Ecosystem.
    pub ecosystem: MutantEcosystem,
    /// Operator token.
    pub operator: String,
    /// Changed region (`file:span`).
    pub region: String,
}

/// Outcome of one mutant against the selected suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutantStatus {
    /// An authoritative (non-flaky) exact-case judge failed.
    Killed,
    /// Every authoritative judge passed.
    Survived,
    /// The mutant could not produce normalized test evidence (for example a
    /// compile error, missing case, or only known-flaky judges). Invalid is
    /// incomplete mutation evidence, never killed.
    Invalid,
}

/// Combine exact-case judges into a mutant status.
///
/// A known-flaky failure must not independently produce [`MutantStatus::Killed`].
/// Callers exclude flaky judges before reporting `authoritative_failed`.
#[must_use]
pub fn authoritative_mutant_status(
    authoritative_failed: bool,
    authoritative_passed: bool,
) -> MutantStatus {
    if authoritative_failed {
        MutantStatus::Killed
    } else if authoritative_passed {
        MutantStatus::Survived
    } else {
        MutantStatus::Invalid
    }
}

/// Per-mutant result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutantResult {
    /// Mutant.
    pub mutant: Mutant,
    /// Killed or survived.
    pub status: MutantStatus,
    /// Tests actually run (selected only).
    pub tests_run: Vec<String>,
}

/// One concrete, reversible source edit. Execution applies exactly one edit
/// in an isolated repository snapshot and never mutates the user's checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMutant {
    /// Stable content-derived identity.
    pub id: String,
    /// Ecosystem whose syntax the edit follows.
    pub ecosystem: MutantEcosystem,
    /// Stable snake-case operator token.
    pub operator: String,
    /// Repository-relative source path.
    pub path: String,
    /// One-based changed line containing the edit.
    pub line: u32,
    /// One-based byte column containing the edit.
    pub column: u32,
    start: usize,
    end: usize,
    original: String,
    replacement: String,
}

/// Refusal to plan or apply an unsafe concrete mutation.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MutationError {
    /// Source no longer matches the immutable plan.
    #[error("mutation source changed after planning")]
    SourceChanged,
    /// A planner input is outside its hard bounds.
    #[error("invalid mutation request: {0}")]
    Invalid(String),
}

impl SourceMutant {
    /// Apply this one edit to the exact source it was planned from.
    ///
    /// # Errors
    ///
    /// Refuses stale source or a non-character byte boundary.
    pub fn apply(&self, source: &str) -> Result<String, MutationError> {
        if self.start > self.end
            || self.end > source.len()
            || !source.is_char_boundary(self.start)
            || !source.is_char_boundary(self.end)
            || source.get(self.start..self.end) != Some(self.original.as_str())
        {
            return Err(MutationError::SourceChanged);
        }
        let mut mutated = source.to_owned();
        mutated.replace_range(self.start..self.end, &self.replacement);
        Ok(mutated)
    }

    /// Proof-domain projection without exposing source bytes.
    #[must_use]
    pub fn mutant(&self) -> Mutant {
        Mutant {
            id: self.id.clone(),
            ecosystem: self.ecosystem,
            operator: self.operator.clone(),
            region: format!("{}:{}:{}", self.path, self.line, self.column),
        }
    }
}

/// Plan concrete TS/JS edits whose token starts on a changed line.
///
/// An empty operator list enables the safe built-in catalogue. Unsupported
/// tokens fail closed instead of silently running a different operator set.
///
/// # Errors
///
/// Rejects an empty/unsafe path, an unknown operator, or a zero budget.
pub fn plan_ts_js_source_mutants(
    path: &str,
    source: &str,
    changed_lines: &BTreeSet<u32>,
    operators: &[String],
    max_mutants: usize,
) -> Result<Vec<SourceMutant>, MutationError> {
    plan_source_mutants(
        MutantEcosystem::TsJs,
        path,
        source,
        changed_lines,
        operators,
        max_mutants,
    )
}

/// Plan concrete Go edits whose token starts on a changed line.
///
/// # Errors
///
/// Rejects an empty/unsafe path, an unknown operator, or a zero budget.
pub fn plan_go_source_mutants(
    path: &str,
    source: &str,
    changed_lines: &BTreeSet<u32>,
    operators: &[String],
    max_mutants: usize,
) -> Result<Vec<SourceMutant>, MutationError> {
    plan_source_mutants(
        MutantEcosystem::Go,
        path,
        source,
        changed_lines,
        operators,
        max_mutants,
    )
}

fn plan_source_mutants(
    ecosystem: MutantEcosystem,
    path: &str,
    source: &str,
    changed_lines: &BTreeSet<u32>,
    operators: &[String],
    max_mutants: usize,
) -> Result<Vec<SourceMutant>, MutationError> {
    let unsafe_path = path.trim().is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path
            .split('/')
            .any(|component| component.is_empty() || matches!(component, "." | ".."));
    if unsafe_path || max_mutants == 0 {
        return Err(MutationError::Invalid(
            "path must be repository-relative and max_mutants must be positive".into(),
        ));
    }
    if source.len() > 4 * 1024 * 1024 {
        return Err(MutationError::Invalid(
            "source exceeds the 4 MiB mutation ceiling".into(),
        ));
    }
    let catalogue: &[&str] = match ecosystem {
        MutantEcosystem::TsJs => &[
            "boundary_flip",
            "equality_flip",
            "bool_flip",
            "logical_flip",
            "off_by_one",
            "remove_branch",
            "remove_sort",
            "wrong_permission",
            "omit_callback",
            "omit_error",
            "collection_boundary",
        ],
        MutantEcosystem::Go => &[
            "err_nil_flip",
            "boundary_flip",
            "return_zero",
            "skip_branch",
            "ignore_context",
            "invert_bool",
        ],
    };
    let requested = if operators.is_empty() {
        catalogue
            .iter()
            .map(|operator| (*operator).to_owned())
            .collect::<BTreeSet<_>>()
    } else {
        operators
            .iter()
            .map(|operator| operator.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>()
    };
    if let Some(unknown) = requested
        .iter()
        .find(|operator| !catalogue.contains(&operator.as_str()))
    {
        return Err(MutationError::Invalid(format!(
            "unknown {ecosystem:?} operator `{unknown}`"
        )));
    }
    let mask = code_mask(source);
    let mut edits = Vec::<PlannedEdit>::new();
    match ecosystem {
        MutantEcosystem::TsJs => plan_ts_js(source, &mask, &requested, &mut edits),
        MutantEcosystem::Go => plan_go(source, &mask, &requested, &mut edits),
    }
    edits.retain(|edit| changed_lines.contains(&line_at(source, edit.start)));
    edits.sort_by(|left, right| {
        (
            left.start,
            left.end,
            left.operator,
            left.replacement.as_str(),
        )
            .cmp(&(
                right.start,
                right.end,
                right.operator,
                right.replacement.as_str(),
            ))
    });
    edits.dedup_by(|left, right| {
        left.start == right.start
            && left.end == right.end
            && left.operator == right.operator
            && left.replacement == right.replacement
    });
    edits.truncate(max_mutants.min(256));
    edits
        .into_iter()
        .map(|edit| source_mutant(ecosystem, path, source, edit))
        .collect()
}

#[derive(Debug)]
struct PlannedEdit {
    operator: &'static str,
    start: usize,
    end: usize,
    replacement: String,
}

fn source_mutant(
    ecosystem: MutantEcosystem,
    path: &str,
    source: &str,
    edit: PlannedEdit,
) -> Result<SourceMutant, MutationError> {
    let original = source
        .get(edit.start..edit.end)
        .ok_or(MutationError::SourceChanged)?
        .to_owned();
    let line = line_at(source, edit.start);
    let line_start = source[..edit.start]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let column =
        u32::try_from(edit.start.saturating_sub(line_start).saturating_add(1)).unwrap_or(u32::MAX);
    let seed = format!(
        "{ecosystem:?}\0{path}\0{}\0{}\0{}\0{}",
        edit.operator, edit.start, original, edit.replacement
    );
    let digest = Sha256::digest(seed.as_bytes());
    Ok(SourceMutant {
        id: format!("mut-{}", hex_prefix(&digest, 16)),
        ecosystem,
        operator: edit.operator.to_owned(),
        path: path.to_owned(),
        line,
        column,
        start: edit.start,
        end: edit.end,
        original,
        replacement: edit.replacement,
    })
}

fn hex_prefix(bytes: &[u8], digits: usize) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(digits);
    for byte in bytes {
        if out.len() == digits {
            break;
        }
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        if out.len() < digits {
            out.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    out
}

fn plan_ts_js(
    source: &str,
    mask: &[bool],
    requested: &BTreeSet<String>,
    edits: &mut Vec<PlannedEdit>,
) {
    if requested.contains("boundary_flip") {
        comparison_edits(source, mask, edits, "boundary_flip", false);
    }
    if requested.contains("equality_flip") {
        replace_patterns(
            source,
            mask,
            edits,
            "equality_flip",
            &[("===", "!=="), ("!==", "===")],
        );
    }
    if requested.contains("bool_flip") {
        word_replacements(
            source,
            mask,
            edits,
            "bool_flip",
            &[("true", "false"), ("false", "true")],
        );
    }
    if requested.contains("logical_flip") {
        replace_patterns(
            source,
            mask,
            edits,
            "logical_flip",
            &[("&&", "||"), ("||", "&&")],
        );
    }
    if requested.contains("off_by_one") {
        replace_patterns(
            source,
            mask,
            edits,
            "off_by_one",
            &[("+1", "-1"), ("-1", "+1")],
        );
    }
    if requested.contains("remove_branch") {
        insert_after_keyword(
            source,
            mask,
            edits,
            "remove_branch",
            "if",
            "false && ",
            true,
        );
    }
    if requested.contains("remove_sort") {
        replace_patterns(
            source,
            mask,
            edits,
            "remove_sort",
            &[(".sort()", ".slice()")],
        );
    }
    if requested.contains("wrong_permission") {
        for word in [
            "canDelete",
            "canWrite",
            "hasPermission",
            "isAllowed",
            "isAuthorized",
        ] {
            insert_before_call(source, mask, edits, "wrong_permission", word, "!");
        }
    }
    if requested.contains("omit_callback") {
        for call in ["callback()", "onComplete()", "onSuccess()", "onDone()"] {
            replace_standalone_call(source, mask, edits, "omit_callback", call, "void 0");
        }
    }
    if requested.contains("omit_error") {
        word_replacements(source, mask, edits, "omit_error", &[("throw", "void")]);
    }
    if requested.contains("collection_boundary") {
        collection_boundary_edits(source, mask, edits);
    }
}

fn plan_go(
    source: &str,
    mask: &[bool],
    requested: &BTreeSet<String>,
    edits: &mut Vec<PlannedEdit>,
) {
    if requested.contains("err_nil_flip") {
        replace_patterns(
            source,
            mask,
            edits,
            "err_nil_flip",
            &[
                ("err != nil", "err == nil"),
                ("err == nil", "err != nil"),
                ("nil != err", "nil == err"),
                ("nil == err", "nil != err"),
            ],
        );
    }
    if requested.contains("boundary_flip") {
        comparison_edits(source, mask, edits, "boundary_flip", false);
    }
    if requested.contains("return_zero") {
        return_zero_edits(source, mask, edits);
    }
    if requested.contains("skip_branch") {
        insert_go_branch_skip(source, mask, edits);
    }
    if requested.contains("ignore_context") {
        replace_patterns(
            source,
            mask,
            edits,
            "ignore_context",
            &[("ctx.Err()", "nil"), ("context.Err()", "nil")],
        );
    }
    if requested.contains("invert_bool") {
        word_replacements(
            source,
            mask,
            edits,
            "invert_bool",
            &[("true", "false"), ("false", "true")],
        );
    }
}

fn comparison_edits(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    _include_equality: bool,
) {
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let pair = source.get(index..index.saturating_add(2));
        let (length, replacement) = match pair {
            Some(">=") => (2, ">"),
            Some("<=") => (2, "<"),
            _ if bytes[index] == b'>' && pair != Some(">>") => (1, ">="),
            _ if bytes[index] == b'<' && pair != Some("<<") => (1, "<="),
            _ => {
                index += 1;
                continue;
            }
        };
        if code_range(mask, index, index + length)
            && !(length == 1 && index > 0 && matches!(bytes[index - 1], b'=' | b'>' | b'<'))
            && (length == 2 || comparison_spacing(bytes, index, index + length))
        {
            edits.push(PlannedEdit {
                operator,
                start: index,
                end: index + length,
                replacement: replacement.into(),
            });
        }
        index += length;
    }
}

fn replace_patterns(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    patterns: &[(&str, &str)],
) {
    replace_patterns_at(source, 0, mask, edits, operator, patterns);
}

fn replace_patterns_at(
    source: &str,
    base: usize,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    patterns: &[(&str, &str)],
) {
    for (pattern, replacement) in patterns {
        for (relative, _) in source.match_indices(pattern) {
            let start = base + relative;
            let end = start + pattern.len();
            if code_range(mask, start, end) {
                edits.push(PlannedEdit {
                    operator,
                    start,
                    end,
                    replacement: (*replacement).into(),
                });
            }
        }
    }
}

fn word_replacements(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    patterns: &[(&str, &str)],
) {
    for (pattern, replacement) in patterns {
        for (start, _) in source.match_indices(pattern) {
            let end = start + pattern.len();
            if code_range(mask, start, end) && word_boundary(source, start, end) {
                edits.push(PlannedEdit {
                    operator,
                    start,
                    end,
                    replacement: (*replacement).into(),
                });
            }
        }
    }
}

fn insert_after_keyword(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    keyword: &str,
    insertion: &str,
    after_open_paren: bool,
) {
    for (start, _) in source.match_indices(keyword) {
        let end = start + keyword.len();
        if !code_range(mask, start, end) || !word_boundary(source, start, end) {
            continue;
        }
        let mut at = end;
        while source
            .as_bytes()
            .get(at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            at += 1;
        }
        if after_open_paren {
            if source.as_bytes().get(at) != Some(&b'(') {
                continue;
            }
            at += 1;
        }
        edits.push(PlannedEdit {
            operator,
            start: at,
            end: at,
            replacement: insertion.into(),
        });
    }
}

fn insert_before_call(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    name: &str,
    insertion: &str,
) {
    for (start, _) in source.match_indices(name) {
        let end = start + name.len();
        let mut next = end;
        while source
            .as_bytes()
            .get(next)
            .is_some_and(u8::is_ascii_whitespace)
        {
            next += 1;
        }
        if code_range(mask, start, end)
            && word_boundary(source, start, end)
            && source.as_bytes().get(next) == Some(&b'(')
            && !start
                .checked_sub(1)
                .and_then(|index| source.as_bytes().get(index))
                .is_some_and(|byte| matches!(byte, b'.' | b'!'))
        {
            edits.push(PlannedEdit {
                operator,
                start,
                end: start,
                replacement: insertion.into(),
            });
        }
    }
}

fn replace_standalone_call(
    source: &str,
    mask: &[bool],
    edits: &mut Vec<PlannedEdit>,
    operator: &'static str,
    call: &str,
    replacement: &str,
) {
    let name_end = call.len().saturating_sub(2);
    for (start, _) in source.match_indices(call) {
        let end = start + call.len();
        if code_range(mask, start, end)
            && word_boundary(source, start, start + name_end)
            && start
                .checked_sub(1)
                .and_then(|index| source.as_bytes().get(index))
                .is_none_or(|byte| *byte != b'.')
        {
            edits.push(PlannedEdit {
                operator,
                start,
                end,
                replacement: replacement.into(),
            });
        }
    }
}

fn insert_go_branch_skip(source: &str, mask: &[bool], edits: &mut Vec<PlannedEdit>) {
    for (start, _) in source.match_indices("if") {
        let end = start + 2;
        if !code_range(mask, start, end) || !word_boundary(source, start, end) {
            continue;
        }
        let line_end = source[end..]
            .find('\n')
            .map_or(source.len(), |relative| end + relative);
        let condition_end = source[end..line_end]
            .find('{')
            .map_or(line_end, |relative| end + relative);
        if source[end..condition_end].contains(';') {
            continue;
        }
        let mut at = end;
        while source
            .as_bytes()
            .get(at)
            .is_some_and(u8::is_ascii_whitespace)
        {
            at += 1;
        }
        edits.push(PlannedEdit {
            operator: "skip_branch",
            start: at,
            end: at,
            replacement: "false && ".into(),
        });
    }
}

fn comparison_spacing(source: &[u8], start: usize, end: usize) -> bool {
    start
        .checked_sub(1)
        .and_then(|index| source.get(index))
        .is_some_and(u8::is_ascii_whitespace)
        && source.get(end).is_some_and(u8::is_ascii_whitespace)
}

fn collection_boundary_edits(source: &str, mask: &[bool], edits: &mut Vec<PlannedEdit>) {
    for (start, _) in source.match_indices(".slice(0,") {
        if !code_range(mask, start, start + 9) {
            continue;
        }
        let mut number_start = start + 9;
        while source
            .as_bytes()
            .get(number_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            number_start += 1;
        }
        let mut end = number_start;
        while source.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        let Some(value) = source
            .get(number_start..end)
            .filter(|value| !value.is_empty())
            .and_then(|value| value.parse::<u64>().ok())
        else {
            continue;
        };
        edits.push(PlannedEdit {
            operator: "collection_boundary",
            start: number_start,
            end,
            replacement: value.saturating_add(1).to_string(),
        });
    }
}

fn return_zero_edits(source: &str, mask: &[bool], edits: &mut Vec<PlannedEdit>) {
    for (start, _) in source.match_indices("return") {
        let keyword_end = start + 6;
        if !code_range(mask, start, keyword_end) || !word_boundary(source, start, keyword_end) {
            continue;
        }
        let mut value_start = keyword_end;
        while source
            .as_bytes()
            .get(value_start)
            .is_some_and(u8::is_ascii_whitespace)
        {
            value_start += 1;
        }
        let tail = &source[value_start..];
        let (length, replacement) = if tail.starts_with("true")
            && word_boundary(source, value_start, value_start + 4)
        {
            (4, "false")
        } else if tail.starts_with("false") && word_boundary(source, value_start, value_start + 5) {
            (5, "true")
        } else {
            let digits = tail.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || tail[..digits].bytes().all(|byte| byte == b'0') {
                continue;
            }
            (digits, "0")
        };
        if code_range(mask, value_start, value_start + length) {
            edits.push(PlannedEdit {
                operator: "return_zero",
                start: value_start,
                end: value_start + length,
                replacement: replacement.into(),
            });
        }
    }
}

fn line_at(source: &str, offset: usize) -> u32 {
    u32::try_from(
        source
            .get(..offset.min(source.len()))
            .unwrap_or(source)
            .split('\n')
            .count(),
    )
    .unwrap_or(u32::MAX)
}

fn word_boundary(source: &str, start: usize, end: usize) -> bool {
    let identifier = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$');
    !start
        .checked_sub(1)
        .and_then(|index| source.as_bytes().get(index))
        .is_some_and(|byte| identifier(*byte))
        && !source
            .as_bytes()
            .get(end)
            .is_some_and(|byte| identifier(*byte))
}

fn code_range(mask: &[bool], start: usize, end: usize) -> bool {
    start <= end && end <= mask.len() && mask[start..end].iter().all(|value| *value)
}

/// Conservative lexical mask: strings and comments are never mutation sites.
/// This is not a parser and creates no code graph; Weavatrix remains the only
/// source-intelligence engine.
fn code_mask(source: &str) -> Vec<bool> {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment,
        Single,
        Double,
        Backtick,
    }
    let bytes = source.as_bytes();
    let mut mask = vec![false; bytes.len()];
    let mut state = State::Code;
    let mut escaped = false;
    let mut index = 0;
    while index < bytes.len() {
        let current = bytes[index];
        let next = bytes.get(index + 1).copied();
        match state {
            State::Code if current == b'/' && next == Some(b'/') => {
                state = State::LineComment;
                index += 2;
                continue;
            }
            State::Code if current == b'/' && next == Some(b'*') => {
                state = State::BlockComment;
                index += 2;
                continue;
            }
            State::Code if current == b'\'' => state = State::Single,
            State::Code if current == b'"' => state = State::Double,
            State::Code if current == b'`' => state = State::Backtick,
            State::Code => mask[index] = true,
            State::LineComment if current == b'\n' => {
                state = State::Code;
                mask[index] = true;
            }
            State::BlockComment if current == b'*' && next == Some(b'/') => {
                state = State::Code;
                index += 2;
                continue;
            }
            State::Single | State::Double | State::Backtick => {
                let delimiter = match state {
                    State::Single => b'\'',
                    State::Double => b'"',
                    State::Backtick => b'`',
                    _ => unreachable!(),
                };
                if current == delimiter && !escaped {
                    state = State::Code;
                }
                escaped = current == b'\\' && !escaped;
                if current != b'\\' {
                    escaped = false;
                }
            }
            State::LineComment | State::BlockComment => {}
        }
        index += 1;
    }
    mask
}

/// Attached to a Proof. Survived > 0 is weakness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MutationSummary {
    /// Killed mutants.
    pub killed: u64,
    /// Survived mutants.
    pub survived: u64,
    /// Mutants that failed before a selected test could judge them.
    pub invalid: u64,
    /// The producer was required but could not execute a comparable suite.
    pub unmeasured: bool,
}

impl MutationSummary {
    /// From individual results.
    #[must_use]
    pub fn from_results(results: &[MutantResult]) -> Self {
        let mut killed = 0_u64;
        let mut survived = 0_u64;
        let mut invalid = 0_u64;
        for item in results {
            match item.status {
                MutantStatus::Killed => killed = killed.saturating_add(1),
                MutantStatus::Survived => survived = survived.saturating_add(1),
                MutantStatus::Invalid => invalid = invalid.saturating_add(1),
            }
        }
        Self {
            killed,
            survived,
            invalid,
            unmeasured: false,
        }
    }
}

/// Whether a selected test detects a mutant.
pub trait MutantOracle {
    /// True when the test fails under the mutant (kills it).
    fn test_fails(&self, mutant_id: &str, test_id: &str) -> bool;
}

/// TS/JS operators for one changed region. Empty region yields none.
#[must_use]
pub fn ts_js_mutants(region: &str) -> Vec<Mutant> {
    if region.is_empty() {
        return Vec::new();
    }
    [
        TsJsOperator::CmpGtGe,
        TsJsOperator::CmpLtLe,
        TsJsOperator::EqNeq,
        TsJsOperator::BoolFlip,
        TsJsOperator::AndOr,
        TsJsOperator::OffByOne,
        TsJsOperator::RemoveBranch,
        TsJsOperator::RemoveSort,
        TsJsOperator::WrongPermission,
        TsJsOperator::OmitCallback,
        TsJsOperator::OmitError,
        TsJsOperator::CollectionBoundary,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, operator)| Mutant {
        id: format!("ts-{index}-{region}"),
        ecosystem: MutantEcosystem::TsJs,
        operator: format!("{operator:?}"),
        region: region.to_owned(),
    })
    .collect()
}

/// Safe Go operators for one changed region.
#[must_use]
pub fn go_mutants(region: &str) -> Vec<Mutant> {
    if region.is_empty() {
        return Vec::new();
    }
    [
        GoOperator::ErrNilFlip,
        GoOperator::BoundaryFlip,
        GoOperator::ReturnZero,
        GoOperator::SkipBranch,
        GoOperator::IgnoreContext,
        GoOperator::InvertBool,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, operator)| Mutant {
        id: format!("go-{index}-{region}"),
        ecosystem: MutantEcosystem::Go,
        operator: format!("{operator:?}"),
        region: region.to_owned(),
    })
    .collect()
}

/// Run mutants against the selected suite only. Other tests are not invoked.
pub fn run_selected_mutants(
    mutants: &[Mutant],
    selected: &[String],
    oracle: &dyn MutantOracle,
) -> Vec<MutantResult> {
    let selected: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    mutants
        .iter()
        .map(|mutant| {
            let mut tests_run = Vec::new();
            let mut killed = false;
            for test in &selected {
                tests_run.push((*test).to_owned());
                if oracle.test_fails(&mutant.id, test) {
                    killed = true;
                }
            }
            MutantResult {
                mutant: mutant.clone(),
                status: if killed {
                    MutantStatus::Killed
                } else {
                    MutantStatus::Survived
                },
                tests_run,
            }
        })
        .collect()
}
