//! `wvq` CLI. Parses argv and calls the command bus. No domain policy here.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use wvq_command_bus::{
    Command, ContextCommand, DebtCommand, ExplainCommand, LiveService, ModelCommand, PlanCommand,
    QualityService, RecordCommand, RecoveryCommand, RunCommand, SelectCommand, SpecCommand,
    VerifyCommand, dispatch,
};

/// Parsed invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliRequest {
    /// Repository root.
    pub repo: PathBuf,
    /// Bus command.
    pub command: Command,
}

/// CLI stdout/stderr + process code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliOutput {
    /// Process exit code.
    pub code: i32,
    /// JSON body (or usage).
    pub stdout: String,
    /// Error text.
    pub stderr: String,
}

/// Usage text for `--help`.
#[must_use]
pub fn usage() -> String {
    "wvq — Weavatrix Quality

Usage:
  wvq [--repo PATH] spec validate [--change ID]
  wvq [--repo PATH] spec seal [--change ID]
  wvq [--repo PATH] analyze [--change ID] [--purpose spec|implementation|review] [--token-budget N]
  wvq [--repo PATH] debt [--change ID] [--base REF] [--head REF|WORKTREE]
  wvq [--repo PATH] select [--change ID] [--base REF] [--head REF|WORKTREE]
  wvq [--repo PATH] recover [--change ID] [--base REF] [--head REF|WORKTREE]
  wvq [--repo PATH] record [--change ID] [--base REF] [--head REF|WORKTREE] [--route /PATH] [--idle-ms N] [--max-events N] [--headless true|false] [--fixtures-json JSON]
  wvq [--repo PATH] model [--change ID] --kind planning|runtime|browser_escape|vision --prompt TEXT
  wvq [--repo PATH] run [--change ID] [--base REF] [--head REF|WORKTREE] [--scope impacted|all] [--evidence-policy standard|minimal|none]
  wvq [--repo PATH] verify [--change ID]
  wvq [--repo PATH] explain <id>
  wvq [--repo PATH] plan [--change ID]
  wvq [--repo PATH] status
"
    .to_owned()
}

/// Parse argv (without the binary name).
///
/// # Errors
///
/// Returns a usage string when the invocation is incomplete or unknown.
pub fn parse_args(args: &[String]) -> Result<CliRequest, String> {
    if args.iter().any(|item| item == "--help" || item == "-h") {
        return Err(usage());
    }
    let mut flags = BTreeMap::new();
    let mut positionals = Vec::new();
    let mut index = 0;
    while index < args.len() {
        let item = &args[index];
        if let Some(name) = item.strip_prefix("--") {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| format!("flag --{name} requires a value"))?;
            if flags.insert(name.to_owned(), value.clone()).is_some() {
                return Err(format!("flag --{name} was supplied more than once"));
            }
            index += 1;
            continue;
        }
        positionals.push(item.clone());
        index += 1;
    }
    if let Some(allowed) = allowed_flags(&positionals)
        && let Some(unknown) = flags.keys().find(|name| !allowed.contains(&name.as_str()))
    {
        return Err(format!(
            "unknown flag --{unknown} for {}",
            positionals.join(" ")
        ));
    }
    let repo = flags
        .get("repo")
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let change = flags
        .get("change")
        .cloned()
        .unwrap_or_else(|| "current".to_owned());
    let base = flags
        .get("base")
        .cloned()
        .unwrap_or_else(|| "HEAD".to_owned());
    let head = flags
        .get("head")
        .cloned()
        .unwrap_or_else(|| "WORKTREE".to_owned());
    let command = match positionals.as_slice() {
        [spec, action] if spec == "spec" && action == "validate" => {
            Command::SpecValidate(SpecCommand { change })
        }
        [spec, action] if spec == "spec" && action == "seal" => {
            Command::SpecSeal(SpecCommand { change })
        }
        [cmd] if cmd == "analyze" => Command::Analyze(ContextCommand {
            change,
            purpose: flags
                .get("purpose")
                .cloned()
                .unwrap_or_else(|| "implementation".to_owned()),
            token_budget: parse_budget(flags.get("token-budget"))?,
        }),
        [cmd] if cmd == "debt" => Command::Debt(DebtCommand { change, base, head }),
        [cmd] if cmd == "select" => Command::Select(SelectCommand { change, base, head }),
        [cmd] if cmd == "recover" => Command::Recovery(RecoveryCommand { change, base, head }),
        [cmd] if cmd == "record" => Command::Record(RecordCommand {
            change,
            base,
            head,
            route: flags
                .get("route")
                .cloned()
                .unwrap_or_else(|| "/".to_owned()),
            fixture_values: parse_fixtures(flags.get("fixtures-json"))?,
            idle_timeout_ms: parse_u64_flag(flags.get("idle-ms"), 3_000, "idle-ms")?,
            max_events: u32::try_from(parse_u64_flag(
                flags.get("max-events"),
                200,
                "max-events",
            )?)
            .map_err(|_| "invalid --max-events value".to_owned())?,
            headless: flags
                .get("headless")
                .map(|value| parse_bool_flag(value, "headless"))
                .transpose()?,
        }),
        [cmd] if cmd == "model" => Command::Model(ModelCommand {
            change,
            kind: required_flag(&flags, "kind")?,
            prompt: required_flag(&flags, "prompt")?,
        }),
        [cmd] if cmd == "run" => Command::Run(RunCommand {
            change,
            scope: flags
                .get("scope")
                .cloned()
                .unwrap_or_else(|| "impacted".to_owned()),
            evidence_policy: flags
                .get("evidence-policy")
                .cloned()
                .unwrap_or_else(|| "standard".to_owned()),
            base,
            head,
        }),
        [cmd] if cmd == "verify" => Command::Verify(VerifyCommand { change }),
        [cmd] if cmd == "plan" => Command::Plan(PlanCommand { change }),
        [cmd] if cmd == "status" => {
            Command::Status(wvq_command_bus::StatusCommand { run_id: None })
        }
        [cmd, id] if cmd == "explain" => Command::Explain(ExplainCommand { id: id.clone() }),
        [] => return Err(usage()),
        other => {
            return Err(format!(
                "unknown command `{}`\n{}",
                other.join(" "),
                usage()
            ));
        }
    };
    Ok(CliRequest { repo, command })
}

fn required_flag(flags: &BTreeMap<String, String>, name: &str) -> Result<String, String> {
    flags
        .get(name)
        .filter(|value| !value.is_empty())
        .cloned()
        .ok_or_else(|| format!("flag --{name} is required"))
}

fn allowed_flags(positionals: &[String]) -> Option<&'static [&'static str]> {
    match positionals {
        [spec, action] if spec == "spec" && matches!(action.as_str(), "validate" | "seal") => {
            Some(&["repo", "change"])
        }
        [cmd] if cmd == "analyze" => Some(&["repo", "change", "purpose", "token-budget"]),
        [cmd] if matches!(cmd.as_str(), "debt" | "select" | "recover") => {
            Some(&["repo", "change", "base", "head"])
        }
        [cmd] if cmd == "record" => Some(&[
            "repo",
            "change",
            "base",
            "head",
            "route",
            "idle-ms",
            "max-events",
            "headless",
            "fixtures-json",
        ]),
        [cmd] if matches!(cmd.as_str(), "verify" | "plan") => Some(&["repo", "change"]),
        [cmd] if cmd == "model" => Some(&["repo", "change", "kind", "prompt"]),
        [cmd] if cmd == "run" => {
            Some(&["repo", "change", "base", "head", "scope", "evidence-policy"])
        }
        [cmd] if cmd == "status" => Some(&["repo"]),
        [cmd, _] if cmd == "explain" => Some(&["repo"]),
        _ => None,
    }
}

fn parse_budget(raw: Option<&String>) -> Result<u64, String> {
    match raw {
        None => Ok(4_000),
        Some(text) => text
            .parse()
            .map_err(|_| format!("invalid --token-budget {text}")),
    }
}

fn parse_u64_flag(raw: Option<&String>, fallback: u64, name: &str) -> Result<u64, String> {
    raw.map_or(Ok(fallback), |text| {
        text.parse().map_err(|_| format!("invalid --{name} {text}"))
    })
}

fn parse_bool_flag(raw: &str, name: &str) -> Result<bool, String> {
    match raw {
        "true" => Ok(true),
        "false" => Ok(false),
        value => Err(format!("invalid --{name} {value}; expected true or false")),
    }
}

fn parse_fixtures(raw: Option<&String>) -> Result<BTreeMap<String, String>, String> {
    raw.map_or_else(
        || Ok(BTreeMap::new()),
        |text| {
            serde_json::from_str(text)
                .map_err(|err| format!("invalid --fixtures-json object: {err}"))
        },
    )
}

/// Run a parsed request against any service.
#[must_use]
pub fn execute(request: &CliRequest, service: &dyn QualityService) -> CliOutput {
    match dispatch(service, request.command.clone()) {
        Ok(reply) => {
            let code = reply.verify_exit_code().unwrap_or(0);
            match serde_json::to_string_pretty(&reply) {
                Ok(stdout) => CliOutput {
                    code,
                    stdout: stdout + "\n",
                    stderr: String::new(),
                },
                Err(err) => CliOutput {
                    code: 1,
                    stdout: String::new(),
                    stderr: format!("{err}\n"),
                },
            }
        }
        Err(err) => CliOutput {
            code: 1,
            stdout: String::new(),
            stderr: format!("{err}\n"),
        },
    }
}

/// Parse + execute using [`LiveService`] for `repo`.
#[must_use]
pub fn run(args: &[String]) -> CliOutput {
    match parse_args(args) {
        Ok(request) => {
            let service = LiveService::new(&request.repo);
            execute(&request, &service)
        }
        Err(message) => usage_output(message),
    }
}

/// Parse + execute against an injected service (tests).
#[must_use]
pub fn run_with(args: &[String], service: &dyn QualityService) -> CliOutput {
    match parse_args(args) {
        Ok(request) => execute(&request, service),
        Err(message) => usage_output(message),
    }
}

fn usage_output(message: String) -> CliOutput {
    if message.starts_with("wvq —") {
        CliOutput {
            code: 0,
            stdout: message,
            stderr: String::new(),
        }
    } else {
        CliOutput {
            code: 1,
            stdout: String::new(),
            stderr: message,
        }
    }
}
