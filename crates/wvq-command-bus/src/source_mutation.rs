//! Isolated changed-line mutation execution.
//!
//! Mutation never edits the user's checkout. A detached worktree is overlaid
//! with the exact working-tree delta, one concrete edit is applied there, and
//! only exact policy-bound tests are executed through the frozen registry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use wvq_proof::{
    MutantEcosystem, MutantStatus, MutationSummary, SourceMutant, obligations_owning_path,
    plan_go_source_mutants, plan_ts_js_source_mutants, surfaces_from_declared_paths,
};
use wvq_runtime::{
    ExecutorId, ExecutorRegistry, PrepareRequest, ProcessLimits, TestStatus,
    discover_executor_targets, parse_go_json, parse_junit,
};
use wvq_spec::QualityContract;

const MAX_MUTANTS: usize = 32;
const MAX_MUTATION_WALL_TIME: Duration = Duration::from_secs(600);
const MAX_MUTANT_TIME: Duration = Duration::from_secs(120);
const MAX_MUTANT_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MUTATION_ARTIFACT_SCHEMA_V: u32 = 1;

#[derive(Debug, Clone)]
pub(crate) struct MutationBinding {
    pub path: String,
    pub runner: String,
    pub case: String,
    pub obligations: BTreeSet<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct MutationPolicy {
    pub obligations: BTreeSet<String>,
    operators: BTreeSet<String>,
    unsupported_operators: BTreeSet<String>,
    all_safe_operators: bool,
    by_obligation: BTreeMap<String, (bool, BTreeSet<String>)>,
}

impl MutationPolicy {
    pub(crate) fn from_contract(contract: &QualityContract) -> Result<Option<Self>, String> {
        let mut policy = Self::default();
        for requirement in &contract.requirements {
            for scenario in &requirement.scenarios {
                let Some(hints) = &scenario.mutation else {
                    continue;
                };
                policy.obligations.extend(
                    scenario
                        .obligations
                        .iter()
                        .map(|obligation| obligation.id.to_string()),
                );
                if hints.operators.is_empty() {
                    policy.all_safe_operators = true;
                }
                for operator in &hints.operators {
                    let operator = operator.trim().to_ascii_lowercase();
                    if operator.is_empty() {
                        return Err("mutation operator names must not be empty".into());
                    }
                    if known_operator(&operator) {
                        policy.operators.insert(operator);
                    } else {
                        policy.unsupported_operators.insert(operator);
                    }
                }
                for obligation in &scenario.obligations {
                    let selection = policy
                        .by_obligation
                        .entry(obligation.id.to_string())
                        .or_default();
                    selection.0 |= hints.operators.is_empty();
                    selection.1.extend(
                        hints
                            .operators
                            .iter()
                            .map(|operator| operator.trim().to_ascii_lowercase())
                            .filter(|operator| known_operator(operator)),
                    );
                }
            }
        }
        Ok((!policy.obligations.is_empty()).then_some(policy))
    }

    fn operators_for(&self, ecosystem: MutantEcosystem) -> Option<Vec<String>> {
        if self.all_safe_operators {
            return Some(Vec::new());
        }
        let operators = self
            .operators
            .iter()
            .filter(|operator| operator_for(ecosystem, operator))
            .cloned()
            .collect::<Vec<_>>();
        (!operators.is_empty()).then_some(operators)
    }

    fn obligations_for(&self, ecosystem: MutantEcosystem, operator: &str) -> Vec<String> {
        self.by_obligation
            .iter()
            .filter(|(_, (all, operators))| {
                (*all && operator_for(ecosystem, operator)) || operators.contains(operator)
            })
            .map(|(obligation, _)| obligation.clone())
            .collect()
    }

    pub(crate) fn summary_for(
        &self,
        evidence: Option<&MutationRunDocument>,
        obligation: &str,
    ) -> Option<MutationSummary> {
        if !self.obligations.contains(obligation) {
            return None;
        }
        evidence.map_or_else(
            || {
                Some(MutationSummary {
                    killed: 0,
                    survived: 0,
                    invalid: 0,
                    unmeasured: true,
                })
            },
            |document| document.summary_for(obligation),
        )
    }

    fn unsupported_limitation(&self) -> Option<String> {
        (!self.unsupported_operators.is_empty()).then(|| {
            format!(
                "custom mutation hints are not built-in source operators: {}",
                self.unsupported_operators
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MutationResultRecord {
    pub id: String,
    pub ecosystem: String,
    pub operator: String,
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub status: String,
    pub obligation: String,
    pub tests_run: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MutationRunDocument {
    pub schema_v: u32,
    pub state: String,
    pub obligations: Vec<String>,
    pub applicable_obligations: Vec<String>,
    pub planned: u64,
    pub killed: u64,
    pub survived: u64,
    pub invalid: u64,
    pub results: Vec<MutationResultRecord>,
    pub limitations: Vec<String>,
    pub runtime_llm_tokens: u64,
}

impl MutationRunDocument {
    pub(crate) fn unmeasured(policy: &MutationPolicy, limitation: String) -> Self {
        let mut limitations = vec![limitation];
        limitations.extend(policy.unsupported_limitation());
        Self {
            schema_v: MUTATION_ARTIFACT_SCHEMA_V,
            state: "unmeasured".into(),
            obligations: policy.obligations.iter().cloned().collect(),
            applicable_obligations: policy.obligations.iter().cloned().collect(),
            planned: 0,
            killed: 0,
            survived: 0,
            invalid: 0,
            results: Vec::new(),
            limitations,
            runtime_llm_tokens: 0,
        }
    }

    pub(crate) fn summary_for(&self, obligation: &str) -> Option<MutationSummary> {
        if self
            .obligations
            .binary_search_by(|candidate| candidate.as_str().cmp(obligation))
            .is_ok()
        {
            let relevant = self
                .results
                .iter()
                .filter(|result| result.obligation == obligation)
                .collect::<Vec<_>>();
            if self
                .applicable_obligations
                .binary_search_by(|candidate| candidate.as_str().cmp(obligation))
                .is_err()
            {
                return None;
            }
            Some(MutationSummary {
                killed: u64::try_from(
                    relevant
                        .iter()
                        .filter(|result| result.status == "killed")
                        .count(),
                )
                .unwrap_or(u64::MAX),
                survived: u64::try_from(
                    relevant
                        .iter()
                        .filter(|result| result.status == "survived")
                        .count(),
                )
                .unwrap_or(u64::MAX),
                invalid: u64::try_from(
                    relevant
                        .iter()
                        .filter(|result| result.status == "invalid")
                        .count(),
                )
                .unwrap_or(u64::MAX),
                unmeasured: relevant.is_empty(),
            })
        } else {
            None
        }
    }

    pub(crate) fn validate(&self, policy: &MutationPolicy) -> Result<(), String> {
        let expected = policy.obligations.iter().cloned().collect::<Vec<_>>();
        if self.obligations != expected {
            return Err("mutation obligations do not match the current quality contract".into());
        }
        if !strictly_sorted_unique(&self.applicable_obligations)
            || self
                .applicable_obligations
                .iter()
                .any(|obligation| !policy.obligations.contains(obligation))
        {
            return Err("mutation applicable_obligations are invalid".into());
        }
        if self.runtime_llm_tokens != 0 {
            return Err("mutation execution must use zero runtime model tokens".into());
        }
        let mut ids = BTreeSet::new();
        let mut killed = 0_u64;
        let mut survived = 0_u64;
        let mut invalid = 0_u64;
        for result in &self.results {
            if !ids.insert(result.id.as_str()) {
                return Err(format!("duplicate mutation result id `{}`", result.id));
            }
            if !self.applicable_obligations.contains(&result.obligation) {
                return Err(format!(
                    "mutation result {} names a non-applicable obligation",
                    result.id
                ));
            }
            let ecosystem = match result.ecosystem.as_str() {
                "ts_js" => MutantEcosystem::TsJs,
                "go" => MutantEcosystem::Go,
                other => return Err(format!("unknown mutation ecosystem `{other}`")),
            };
            if !operator_for(ecosystem, &result.operator)
                || !policy
                    .obligations_for(ecosystem, &result.operator)
                    .contains(&result.obligation)
            {
                return Err(format!(
                    "mutation result {} is not authorized by quality.yaml",
                    result.id
                ));
            }
            if safe_relative(&result.path).is_err() || result.line == 0 || result.column == 0 {
                return Err(format!(
                    "mutation result {} has an invalid region",
                    result.id
                ));
            }
            match result.status.as_str() {
                "killed" => killed = killed.saturating_add(1),
                "survived" => survived = survived.saturating_add(1),
                "invalid" => invalid = invalid.saturating_add(1),
                other => return Err(format!("unknown mutation result status `{other}`")),
            }
        }
        if (self.killed, self.survived, self.invalid) != (killed, survived, invalid) {
            return Err("mutation result counters do not match their records".into());
        }
        let state_is_valid = match self.state.as_str() {
            "not_applicable" => {
                self.planned == 0
                    && self.results.is_empty()
                    && self.applicable_obligations.is_empty()
            }
            "unmeasured" => {
                killed == 0
                    && survived == 0
                    && (self.planned == 0 || invalid > 0)
                    && !self.applicable_obligations.is_empty()
            }
            "measured" => self.planned > 0 && killed.saturating_add(survived) > 0,
            _ => false,
        };
        if !state_is_valid {
            return Err(format!(
                "mutation state `{}` contradicts its evidence",
                self.state
            ));
        }
        Ok(())
    }
}

fn strictly_sorted_unique(values: &[String]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

pub(crate) struct MutationRunRequest<'a> {
    pub repo: &'a Path,
    pub head_commit: &'a str,
    pub merge_base: &'a str,
    pub head_is_worktree: bool,
    pub added_files: &'a [String],
    pub changed_files: &'a [String],
    pub bindings: &'a [MutationBinding],
    pub policy: &'a MutationPolicy,
    pub executors: &'a ExecutorRegistry,
    pub cancel: Arc<AtomicBool>,
}

#[allow(clippy::too_many_lines)]
pub(crate) fn execute_source_mutation(
    request: &MutationRunRequest<'_>,
) -> Result<MutationRunDocument, String> {
    let mut limitations = request
        .policy
        .unsupported_limitation()
        .into_iter()
        .collect::<Vec<_>>();
    let mut planned = Vec::new();
    for path in request
        .added_files
        .iter()
        .chain(request.changed_files.iter())
        .filter(|path| !looks_like_test(path))
        .take(64)
    {
        let Some(ecosystem) = ecosystem(path) else {
            continue;
        };
        let source = std::fs::read_to_string(request.repo.join(path))
            .map_err(|error| format!("cannot read mutation source {path}: {error}"))?;
        let lines = if request.added_files.contains(path) {
            (1..=u32::try_from(source.lines().count()).unwrap_or(u32::MAX)).collect()
        } else {
            changed_lines(
                request.repo,
                request.merge_base,
                request.head_commit,
                request.head_is_worktree,
                path,
            )?
        };
        let remaining = MAX_MUTANTS.saturating_sub(planned.len());
        if remaining == 0 {
            limitations.push(format!(
                "mutation plan hit the {MAX_MUTANTS}-mutant ceiling"
            ));
            break;
        }
        let Some(operators) = request.policy.operators_for(ecosystem) else {
            continue;
        };
        let mut source_mutants = match ecosystem {
            MutantEcosystem::TsJs => {
                plan_ts_js_source_mutants(path, &source, &lines, &operators, remaining)
            }
            MutantEcosystem::Go => {
                plan_go_source_mutants(path, &source, &lines, &operators, remaining)
            }
        }
        .map_err(|error| error.to_string())?;
        planned.append(&mut source_mutants);
    }

    let mut obligations = request
        .policy
        .obligations
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    obligations.sort();
    if planned.is_empty() {
        return Ok(MutationRunDocument {
            schema_v: MUTATION_ARTIFACT_SCHEMA_V,
            state: "not_applicable".into(),
            obligations,
            applicable_obligations: Vec::new(),
            planned: 0,
            killed: 0,
            survived: 0,
            invalid: 0,
            results: Vec::new(),
            limitations,
            runtime_llm_tokens: 0,
        });
    }

    let surfaces = surfaces_from_declared_paths(
        &request
            .bindings
            .iter()
            .map(|binding| (binding.path.clone(), binding.obligations.clone()))
            .collect::<Vec<_>>(),
    );
    let applicable_obligations = planned
        .iter()
        .flat_map(|mutant| {
            obligations_owning_path(
                &surfaces,
                &mutant.path,
                &request
                    .policy
                    .obligations_for(mutant.ecosystem, &mutant.operator),
            )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();

    let mut workspace = MutationWorkspace::create(request.repo, request.head_commit)?;
    workspace.overlay_worktree(request.repo, request.head_commit)?;
    workspace.link_dependencies(request.repo, request.bindings)?;
    let targets = discover_executor_targets(&workspace.path).map_err(|error| error.to_string())?;
    let mut results = Vec::new();
    let mutation_started = Instant::now();
    let mut wall_limit_reached = false;
    let mut decision_limit_reached = false;
    for mutant in &planned {
        if mutation_started.elapsed() >= MAX_MUTATION_WALL_TIME {
            wall_limit_reached = true;
            break;
        }
        let original = std::fs::read_to_string(workspace.path.join(&mutant.path))
            .map_err(|error| format!("cannot read isolated source {}: {error}", mutant.path))?;
        let mutated = mutant.apply(&original).map_err(|error| error.to_string())?;
        std::fs::write(workspace.path.join(&mutant.path), mutated)
            .map_err(|error| format!("cannot write isolated mutant {}: {error}", mutant.id))?;
        for obligation in obligations_owning_path(
            &surfaces,
            &mutant.path,
            &request
                .policy
                .obligations_for(mutant.ecosystem, &mutant.operator),
        ) {
            if mutation_started.elapsed() >= MAX_MUTATION_WALL_TIME {
                wall_limit_reached = true;
                break;
            }
            if results.len() >= 128 {
                decision_limit_reached = true;
                break;
            }
            results.push(execute_one(
                &workspace.path,
                mutant,
                request.bindings,
                &obligation,
                &targets,
                request.executors,
                &request.cancel,
            ));
        }
        std::fs::write(workspace.path.join(&mutant.path), original)
            .map_err(|error| format!("cannot restore isolated source {}: {error}", mutant.path))?;
        if wall_limit_reached || decision_limit_reached {
            break;
        }
    }
    if wall_limit_reached {
        limitations.push("mutation execution hit the 600-second wall-clock ceiling".into());
    }
    if decision_limit_reached {
        limitations.push("mutation execution hit the 128 obligation-case ceiling".into());
    }
    let killed = results
        .iter()
        .filter(|result| result.status == "killed")
        .count();
    let survived = results
        .iter()
        .filter(|result| result.status == "survived")
        .count();
    let invalid = results
        .iter()
        .filter(|result| result.status == "invalid")
        .count();
    if invalid > 0 {
        limitations.push(format!(
            "{invalid} mutants produced no exact normalized test decision"
        ));
    }
    Ok(MutationRunDocument {
        schema_v: MUTATION_ARTIFACT_SCHEMA_V,
        state: if invalid == results.len() {
            "unmeasured".into()
        } else {
            "measured".into()
        },
        obligations,
        applicable_obligations,
        planned: u64::try_from(planned.len()).unwrap_or(u64::MAX),
        killed: u64::try_from(killed).unwrap_or(u64::MAX),
        survived: u64::try_from(survived).unwrap_or(u64::MAX),
        invalid: u64::try_from(invalid).unwrap_or(u64::MAX),
        results,
        limitations,
        runtime_llm_tokens: 0,
    })
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn execute_one(
    workspace: &Path,
    mutant: &SourceMutant,
    bindings: &[MutationBinding],
    obligation: &str,
    targets: &[wvq_runtime::ExecutorTarget],
    executors: &ExecutorRegistry,
    cancel: &Arc<AtomicBool>,
) -> MutationResultRecord {
    let mut tests_run = Vec::new();
    let mut saw_kill = false;
    let mut invalid = false;
    let mut requests =
        BTreeMap::<(String, String, String, String), (&wvq_runtime::ExecutorTarget, &str)>::new();
    for binding in bindings.iter().filter(|binding| {
        binding.obligations.contains(obligation)
            && binding_supported(mutant.ecosystem, &binding.runner)
    }) {
        let binding_path = workspace.join(&binding.path);
        let source_path = workspace.join(&mutant.path);
        let Some(target) = targets
            .iter()
            .filter(|target| {
                binding_path.starts_with(&target.cwd) && source_path.starts_with(&target.cwd)
            })
            .max_by_key(|target| target.cwd.components().count())
        else {
            continue;
        };
        if target.executor.as_str() != binding.runner {
            continue;
        }
        let filter = if binding.runner == "go-test" {
            String::new()
        } else {
            binding_path
                .strip_prefix(&target.cwd)
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_default()
        };
        requests.insert(
            (
                binding.runner.clone(),
                target.cwd.display().to_string(),
                filter,
                binding.case.clone(),
            ),
            (target, binding.case.as_str()),
        );
    }
    if requests.is_empty() {
        invalid = true;
    }
    for ((runner, _, filter, _), (target, case)) in requests {
        tests_run.push(format!("{runner}#{case}"));
        let evidence_dir = target.cwd.join(".weavatrix-quality");
        let _ = std::fs::create_dir_all(&evidence_dir);
        let junit = evidence_dir.join("junit.xml");
        let _ = std::fs::remove_file(&junit);
        let Ok(executor) = ExecutorId::new(&runner) else {
            invalid = true;
            continue;
        };
        let prepared = executors.prepare(PrepareRequest {
            executor,
            cwd: target.cwd.clone(),
            filters: (!filter.is_empty()).then_some(filter).into_iter().collect(),
            exact_case: Some(case.to_owned()),
            extra: BTreeMap::new(),
            limits: ProcessLimits {
                deadline: MAX_MUTANT_TIME,
                max_output_bytes: MAX_MUTANT_OUTPUT_BYTES,
            },
            cancel: Arc::clone(cancel),
        });
        let Ok(prepared) = prepared else {
            invalid = true;
            continue;
        };
        let Ok(executed) = executors.execute(&prepared) else {
            invalid = true;
            continue;
        };
        let normalized = if runner == "go-test" {
            std::str::from_utf8(&executed.stdout)
                .ok()
                .and_then(|stdout| parse_go_json(stdout).ok())
        } else {
            std::fs::read_to_string(&junit)
                .ok()
                .and_then(|xml| parse_junit(&xml).ok())
        };
        let Some(normalized) = normalized else {
            invalid = true;
            continue;
        };
        let exact = normalized
            .cases
            .iter()
            .filter(|candidate| candidate.name == case)
            .collect::<Vec<_>>();
        if exact.len() != 1 {
            invalid = true;
            continue;
        }
        match exact[0].status {
            TestStatus::Fail | TestStatus::Error => saw_kill = true,
            TestStatus::Pass if executed.status_code == Some(0) => {}
            TestStatus::Pass | TestStatus::Skip => invalid = true,
        }
    }
    tests_run.sort();
    tests_run.dedup();
    let status = if saw_kill {
        MutantStatus::Killed
    } else if invalid {
        MutantStatus::Invalid
    } else {
        MutantStatus::Survived
    };
    MutationResultRecord {
        id: format!("{}--{obligation}", mutant.id),
        ecosystem: match mutant.ecosystem {
            MutantEcosystem::TsJs => "ts_js",
            MutantEcosystem::Go => "go",
        }
        .into(),
        operator: mutant.operator.clone(),
        path: mutant.path.clone(),
        line: mutant.line,
        column: mutant.column,
        status: match status {
            MutantStatus::Killed => "killed",
            MutantStatus::Survived => "survived",
            MutantStatus::Invalid => "invalid",
        }
        .into(),
        obligation: obligation.to_owned(),
        tests_run,
    }
}

fn known_operator(operator: &str) -> bool {
    operator_for(MutantEcosystem::TsJs, operator) || operator_for(MutantEcosystem::Go, operator)
}

fn operator_for(ecosystem: MutantEcosystem, operator: &str) -> bool {
    match ecosystem {
        MutantEcosystem::TsJs => matches!(
            operator,
            "boundary_flip"
                | "equality_flip"
                | "bool_flip"
                | "logical_flip"
                | "off_by_one"
                | "remove_branch"
                | "remove_sort"
                | "wrong_permission"
                | "omit_callback"
                | "omit_error"
                | "collection_boundary"
        ),
        MutantEcosystem::Go => matches!(
            operator,
            "err_nil_flip"
                | "boundary_flip"
                | "return_zero"
                | "skip_branch"
                | "ignore_context"
                | "invert_bool"
        ),
    }
}

fn ecosystem(path: &str) -> Option<MutantEcosystem> {
    let extension = Path::new(path).extension().and_then(|value| value.to_str());
    if extension.is_some_and(|value| value.eq_ignore_ascii_case("go")) {
        Some(MutantEcosystem::Go)
    } else if ["ts", "tsx", "js", "jsx", "mjs", "cjs"]
        .iter()
        .any(|expected| extension.is_some_and(|value| value.eq_ignore_ascii_case(expected)))
    {
        Some(MutantEcosystem::TsJs)
    } else {
        None
    }
}

fn looks_like_test(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with("_test.go")
        || lower.contains("/tests/")
        || lower.contains(".test.")
        || lower.contains(".spec.")
        || lower.contains(".stories.")
}

fn binding_supported(ecosystem: MutantEcosystem, runner: &str) -> bool {
    match ecosystem {
        MutantEcosystem::Go => runner == "go-test",
        MutantEcosystem::TsJs => matches!(
            runner,
            "vitest" | "storybook-vitest" | "storybook-vitest-v8"
        ),
    }
}

fn changed_lines(
    repo: &Path,
    merge_base: &str,
    head_commit: &str,
    head_is_worktree: bool,
    path: &str,
) -> Result<BTreeSet<u32>, String> {
    let mut args = vec![
        "diff".to_owned(),
        "--unified=0".to_owned(),
        "--no-color".to_owned(),
        merge_base.to_owned(),
    ];
    if !head_is_worktree {
        args.push(head_commit.to_owned());
    }
    args.extend(["--".to_owned(), path.to_owned()]);
    let raw = String::from_utf8(git(repo, &args)?)
        .map_err(|error| format!("Git diff for {path} is not UTF-8: {error}"))?;
    let mut lines = BTreeSet::new();
    for header in raw.lines().filter(|line| line.starts_with("@@ ")) {
        let Some(head) = header
            .split_whitespace()
            .find(|field| field.starts_with('+'))
        else {
            continue;
        };
        let mut fields = head.trim_start_matches('+').split(',');
        let start = fields
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);
        let count = fields
            .next()
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);
        for line in start..start.saturating_add(count) {
            if line > 0 {
                lines.insert(line);
            }
        }
    }
    Ok(lines)
}

struct MutationWorkspace {
    repo: PathBuf,
    owner: PathBuf,
    path: PathBuf,
    linked_directories: Vec<PathBuf>,
}

impl MutationWorkspace {
    fn create(repo: &Path, head_commit: &str) -> Result<Self, String> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let owner = std::env::temp_dir().join(format!("wvq-mut-{}-{nanos}", std::process::id()));
        let path = owner.join("repo");
        std::fs::create_dir_all(&owner).map_err(|error| error.to_string())?;
        git(
            repo,
            &[
                "worktree".into(),
                "add".into(),
                "--detach".into(),
                path.display().to_string(),
                head_commit.into(),
            ],
        )?;
        Ok(Self {
            repo: repo.to_path_buf(),
            owner,
            path,
            linked_directories: Vec::new(),
        })
    }

    fn overlay_worktree(&self, repo: &Path, head_commit: &str) -> Result<(), String> {
        let mut paths = nul_paths(&git(
            repo,
            &[
                "diff".into(),
                "--no-renames".into(),
                "--name-only".into(),
                "-z".into(),
                head_commit.into(),
                "--".into(),
            ],
        )?)?;
        paths.extend(nul_paths(&git(
            repo,
            &[
                "ls-files".into(),
                "--others".into(),
                "--exclude-standard".into(),
                "-z".into(),
            ],
        )?)?);
        paths.sort();
        paths.dedup();
        for relative in paths {
            let relative = safe_relative(&relative)?;
            let source = repo.join(&relative);
            let target = self.path.join(&relative);
            if source.is_file() {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                std::fs::copy(&source, &target)
                    .map_err(|error| format!("cannot overlay {}: {error}", relative.display()))?;
            } else if target.exists() {
                std::fs::remove_file(&target).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn link_dependencies(
        &mut self,
        repo: &Path,
        bindings: &[MutationBinding],
    ) -> Result<(), String> {
        let mut roots = BTreeSet::from([PathBuf::new()]);
        for binding in bindings {
            let binding_path = repo.join(&binding.path);
            let Some(parent) = binding_path.parent() else {
                continue;
            };
            for ancestor in parent.ancestors() {
                if !ancestor.starts_with(repo) {
                    break;
                }
                if ancestor.join("node_modules").is_dir() {
                    roots.insert(
                        ancestor
                            .strip_prefix(repo)
                            .map_err(|error| error.to_string())?
                            .to_path_buf(),
                    );
                    break;
                }
            }
        }
        for root in roots {
            let source = repo.join(&root).join("node_modules");
            let target = self.path.join(&root).join("node_modules");
            if source.is_dir() && !target.exists() {
                link_directory(&source, &target)?;
                self.linked_directories.push(target);
            }
        }
        Ok(())
    }
}

impl Drop for MutationWorkspace {
    fn drop(&mut self) {
        for link in self.linked_directories.iter().rev() {
            unlink_directory(link);
        }
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&self.path)
            .current_dir(&self.repo)
            .output();
        let _ = Command::new("git")
            .args(["worktree", "prune"])
            .current_dir(&self.repo)
            .output();
        let _ = std::fs::remove_dir_all(&self.owner);
    }
}

fn safe_relative(raw: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("unsafe repository path `{raw}`"));
    }
    Ok(path.to_path_buf())
}

fn nul_paths(raw: &[u8]) -> Result<Vec<String>, String> {
    raw.split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
        .map(|path| {
            std::str::from_utf8(path)
                .map(str::to_owned)
                .map_err(|error| format!("Git path is not UTF-8: {error}"))
        })
        .collect()
}

fn git(repo: &Path, args: &[String]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|error| format!("cannot run Git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Git {} failed: {}",
            args.first().map_or("operation", String::as_str),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

#[cfg(windows)]
fn link_directory(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let output = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "New-Item -ItemType Junction -Path $env:WVQ_LINK_TARGET -Target $env:WVQ_LINK_SOURCE | Out-Null",
        ])
        .env("WVQ_LINK_TARGET", target)
        .env("WVQ_LINK_SOURCE", source)
        .output()
        .map_err(|error| format!("cannot create dependency junction: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cannot create dependency junction: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn link_directory(source: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn unlink_directory(target: &Path) {
    let _ = std::fs::remove_dir(target);
}

#[cfg(unix)]
fn unlink_directory(target: &Path) {
    let _ = std::fs::remove_file(target);
}

#[cfg(test)]
mod tests {
    use super::{MutationPolicy, MutationResultRecord, MutationRunDocument};

    fn policy() -> MutationPolicy {
        let mut policy = MutationPolicy::default();
        policy.obligations.extend(["admin".into(), "viewer".into()]);
        policy.operators.insert("boundary_flip".into());
        policy.by_obligation.insert(
            "admin".into(),
            (false, ["boundary_flip".into()].into_iter().collect()),
        );
        policy.by_obligation.insert(
            "viewer".into(),
            (false, ["boundary_flip".into()].into_iter().collect()),
        );
        policy
    }

    fn result(obligation: &str, status: &str) -> MutationResultRecord {
        MutationResultRecord {
            id: format!("mut-1--{obligation}"),
            ecosystem: "go".into(),
            operator: "boundary_flip".into(),
            path: "limit.go".into(),
            line: 4,
            column: 15,
            status: status.into(),
            obligation: obligation.into(),
            tests_run: vec![format!("go-test#Test{obligation}")],
        }
    }

    #[test]
    fn mutation_strength_is_attributed_to_the_obligation_its_exact_test_judged() {
        let document = MutationRunDocument {
            schema_v: 1,
            state: "measured".into(),
            obligations: vec!["admin".into(), "viewer".into()],
            applicable_obligations: vec!["admin".into(), "viewer".into()],
            planned: 1,
            killed: 1,
            survived: 1,
            invalid: 0,
            results: vec![result("admin", "killed"), result("viewer", "survived")],
            limitations: Vec::new(),
            runtime_llm_tokens: 0,
        };
        let admin = document.summary_for("admin").unwrap();
        let viewer = document.summary_for("viewer").unwrap();
        assert_eq!((admin.killed, admin.survived), (1, 0));
        assert_eq!((viewer.killed, viewer.survived), (0, 1));
    }

    #[test]
    fn applicable_obligation_without_an_exact_result_stays_unmeasured() {
        let document = MutationRunDocument {
            schema_v: 1,
            state: "measured".into(),
            obligations: vec!["admin".into(), "viewer".into()],
            applicable_obligations: vec!["admin".into(), "viewer".into()],
            planned: 1,
            killed: 1,
            survived: 0,
            invalid: 0,
            results: vec![result("admin", "killed")],
            limitations: vec!["execution ceiling".into()],
            runtime_llm_tokens: 0,
        };

        let viewer = document.summary_for("viewer").unwrap();
        assert!(viewer.unmeasured);
        assert_eq!((viewer.killed, viewer.survived, viewer.invalid), (0, 0, 0));
    }

    #[test]
    fn missing_required_mutation_artifact_is_unmeasured() {
        let summary = policy().summary_for(None, "viewer").unwrap();
        assert!(summary.unmeasured);
        assert_eq!(
            (summary.killed, summary.survived, summary.invalid),
            (0, 0, 0)
        );
    }

    #[test]
    fn mutation_document_validation_refuses_forged_counters() {
        let mut document = MutationRunDocument {
            schema_v: 1,
            state: "measured".into(),
            obligations: vec!["admin".into(), "viewer".into()],
            applicable_obligations: vec!["admin".into()],
            planned: 1,
            killed: 1,
            survived: 0,
            invalid: 0,
            results: vec![result("admin", "killed")],
            limitations: Vec::new(),
            runtime_llm_tokens: 0,
        };
        assert!(document.validate(&policy()).is_ok());
        document.killed = 0;
        assert_eq!(
            document.validate(&policy()).unwrap_err(),
            "mutation result counters do not match their records"
        );
    }
}
