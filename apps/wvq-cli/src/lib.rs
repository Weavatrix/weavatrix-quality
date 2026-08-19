//! `wvq` CLI. Parses argv and calls the command bus. No domain policy here.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use wvq_command_bus::{
    Command, ContextCommand, DebtCommand, ExplainCommand, LiveService, PlanCommand, QualityService,
    RunCommand, SelectCommand, SpecCommand, VerifyCommand, dispatch,
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
  wvq [--repo PATH] debt [--change ID]
  wvq [--repo PATH] select [--change ID]
  wvq [--repo PATH] run [--change ID] [--scope impacted|all] [--evidence-policy standard|minimal|none]
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
            flags.insert(name.to_owned(), value.clone());
            index += 1;
            continue;
        }
        positionals.push(item.clone());
        index += 1;
    }
    let repo = flags
        .get("repo")
        .map_or_else(|| PathBuf::from("."), PathBuf::from);
    let change = flags
        .get("change")
        .cloned()
        .unwrap_or_else(|| "current".to_owned());
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
        [cmd] if cmd == "debt" => Command::Debt(DebtCommand { change }),
        [cmd] if cmd == "select" => Command::Select(SelectCommand { change }),
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

fn parse_budget(raw: Option<&String>) -> Result<u64, String> {
    match raw {
        None => Ok(4_000),
        Some(text) => text
            .parse()
            .map_err(|_| format!("invalid --token-budget {text}")),
    }
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
