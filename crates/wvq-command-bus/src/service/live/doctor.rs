//! Read-only repository discovery. Detection is never authority.

use super::super::access::*;
use super::LiveService;
use wvq_runtime::discover_executor_targets;

impl LiveService {
    pub(in crate::service) fn doctor(&self, cmd: &DoctorCommand) -> Result<DoctorReply, BusError> {
        let _ = cmd;
        if !self.repo.is_dir() {
            return Err(BusError::InvalidInput(format!(
                "doctor requires a directory, got {}",
                self.repo.display()
            )));
        }

        let policy_path = self.repo.join(".weavatrix-quality").join("config.yaml");
        let policy_present = policy_path.is_file();
        let (mut policy_loadable, mut policy_error) = policy_status(&policy_path, policy_present);

        let bindings = match load_test_bindings(&self.repo) {
            Ok(bindings) => bindings,
            Err(error) => {
                policy_loadable = false;
                if policy_error.is_none() {
                    policy_error = Some(error.to_string());
                }
                Vec::new()
            }
        };

        let (browser_configured, browser_origin, browser_error) =
            match load_browser_runtime_with(&self.repo, None) {
                Ok(Some(policy)) => (true, Some(policy.base_url), None),
                Ok(None) => (false, None, None),
                Err(error) => (false, None, Some(error.to_string())),
            };
        if let Some(error) = browser_error {
            policy_loadable = false;
            if policy_error.is_none() {
                policy_error = Some(error);
            }
        }

        let targets = discover_executor_targets(&self.repo)
            .map_err(|err| BusError::Runtime(format!("cannot discover executors: {err}")))?;
        let runners = targets
            .iter()
            .map(|target| DoctorRunner {
                executor: target.executor.as_str().to_owned(),
                cwd: relative_cwd(&self.repo, &target.cwd),
            })
            .collect::<Vec<_>>();
        let openspec_dir = self.repo.join("openspec").join("changes");
        let mut reply = DoctorReply {
            authority: false,
            policy_present,
            policy_loadable,
            policy_error,
            openspec_present: openspec_dir.is_dir(),
            openspec_changes: openspec_change_names(&openspec_dir),
            ecosystems: ecosystems_from(&targets),
            runners,
            bindings: bindings
                .iter()
                .map(|binding| DoctorBinding {
                    path: binding.path.clone(),
                    runner: binding.runner.clone(),
                    obligations: binding.obligations.iter().cloned().collect(),
                })
                .collect(),
            browser_configured,
            browser_origin,
            suggested_next: Vec::new(),
            runtime_llm_tokens: 0,
        };
        reply.suggested_next = suggestions(&reply);
        Ok(reply)
    }
}

fn policy_status(path: &Path, present: bool) -> (bool, Option<String>) {
    if !present {
        return (false, None);
    }
    match std::fs::read_to_string(path) {
        Ok(raw) => match serde_yaml::from_str::<serde_yaml::Value>(&raw) {
            Ok(value) => match value
                .get("quality_policy_v")
                .and_then(serde_yaml::Value::as_u64)
            {
                Some(1) => (true, None),
                Some(other) => (false, Some(format!("unknown quality_policy_v {other}"))),
                None => (
                    false,
                    Some("quality policy is missing quality_policy_v".into()),
                ),
            },
            Err(error) => (false, Some(error.to_string())),
        },
        Err(error) => (false, Some(error.to_string())),
    }
}

fn openspec_change_names(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

fn relative_cwd(repo: &Path, cwd: &Path) -> String {
    cwd.strip_prefix(repo).map_or_else(
        |_| cwd.display().to_string(),
        |path| {
            let rendered = path.to_string_lossy().replace('\\', "/");
            if rendered.is_empty() {
                ".".into()
            } else {
                rendered
            }
        },
    )
}

fn ecosystems_from(targets: &[wvq_runtime::ExecutorTarget]) -> Vec<String> {
    let mut ecosystems = BTreeSet::new();
    for target in targets {
        match target.executor.as_str() {
            "cargo-test" => {
                ecosystems.insert("rust".into());
            }
            "go-test" => {
                ecosystems.insert("go".into());
            }
            "vitest" | "jest" | "bun-test" | "npm-test" => {
                ecosystems.insert("javascript".into());
            }
            "playwright" => {
                ecosystems.insert("playwright".into());
            }
            id if id.starts_with("storybook") => {
                ecosystems.insert("storybook".into());
            }
            _ => {}
        }
        if package_has_react(&target.cwd) {
            ecosystems.insert("react".into());
        }
    }
    ecosystems.into_iter().collect()
}

fn package_has_react(dir: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(dir.join("package.json")) else {
        return false;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return false;
    };
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .any(|key| {
            json.get(*key)
                .and_then(serde_json::Value::as_object)
                .is_some_and(|deps| deps.contains_key("react"))
        })
}

fn suggestions(reply: &DoctorReply) -> Vec<String> {
    let mut next = Vec::new();
    if !reply.policy_present {
        next.push("wvq init".into());
    } else if !reply.policy_loadable {
        next.push("fix .weavatrix-quality/config.yaml; unknown versions fail closed".into());
    }
    if !reply.openspec_present {
        next.push("add an OpenSpec change; doctor will not invent one".into());
    }
    if reply.runners.is_empty() {
        next.push("no registered executor was discovered".into());
    } else if reply.bindings.is_empty() {
        next.push(
            "declare test_bindings in .weavatrix-quality/config.yaml when a case is known".into(),
        );
    }
    if reply.policy_loadable && reply.openspec_present {
        next.push("wvq verify --observe-only true".into());
    }
    next
}
