#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use wvq_bench::run_live_shadow;
use wvq_command_bus::LiveService;

const USAGE: &str = "wvq-bench — live impacted-vs-full shadow measurement

Usage:
  wvq-bench --repo PATH --change ID --base REF [--head REF|WORKTREE] [--evidence-policy standard|minimal|none]
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1)) {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) if error == USAGE => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<String, String> {
    let args = args.into_iter().collect::<Vec<_>>();
    if args.iter().any(|item| item == "--help" || item == "-h") {
        return Err(USAGE.into());
    }
    let flags = parse_flags(&args)?;
    let repo = PathBuf::from(required(&flags, "repo")?);
    let change = required(&flags, "change")?;
    let base = required(&flags, "base")?;
    let head = flags.get("head").map_or("WORKTREE", String::as_str);
    let evidence_policy = flags
        .get("evidence-policy")
        .map_or("minimal", String::as_str);
    let service = LiveService::new(repo);
    let report = run_live_shadow(&service, change, base, head, evidence_policy)
        .map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&report).map_err(|error| error.to_string())
}

fn parse_flags(args: &[String]) -> Result<BTreeMap<String, String>, String> {
    let allowed = ["repo", "change", "base", "head", "evidence-policy"];
    let mut flags = BTreeMap::new();
    let mut index = 0;
    while index < args.len() {
        let name = args[index]
            .strip_prefix("--")
            .ok_or_else(|| format!("unexpected positional argument `{}`\n{USAGE}", args[index]))?;
        if !allowed.contains(&name) {
            return Err(format!("unknown flag --{name}\n{USAGE}"));
        }
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("flag --{name} requires a value\n{USAGE}"))?;
        if flags.insert(name.to_owned(), value.clone()).is_some() {
            return Err(format!(
                "flag --{name} was supplied more than once\n{USAGE}"
            ));
        }
        index += 1;
    }
    Ok(flags)
}

fn required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, String> {
    flags
        .get(name)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or_else(|| format!("flag --{name} is required\n{USAGE}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_are_bounded_and_required() {
        let parsed = parse_flags(&[
            "--repo".into(),
            ".".into(),
            "--change".into(),
            "live".into(),
            "--base".into(),
            "HEAD".into(),
        ])
        .unwrap();
        assert_eq!(required(&parsed, "change").unwrap(), "live");
        assert!(parse_flags(&["--shell".into(), "echo".into()]).is_err());
        assert!(parse_flags(&["positional".into()]).is_err());
    }
}
