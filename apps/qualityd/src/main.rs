//! `qualityd` binary. Serves the local Quality Studio cockpit and API.

#![forbid(unsafe_code)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;

use qualityd::{Studio, serve};
use wvq_command_bus::{LiveService, QualityService};
use wvq_store::Store;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|item| item == "--help" || item == "-h") {
        println!(
            "qualityd — Weavatrix Quality Studio\n\nUsage:\n  qualityd [--repo PATH] [--addr HOST:PORT] [--recovery-change ID] [--protection-change ID] [--base REF] [--head REF|WORKTREE]\n"
        );
        return ExitCode::SUCCESS;
    }
    let repo = flag(&args, "--repo").map_or_else(|| PathBuf::from("."), PathBuf::from);
    let addr = flag(&args, "--addr").unwrap_or_else(|| DEFAULT_ADDR.to_owned());

    let store = match Store::open(&repo) {
        Ok(store) => store,
        Err(err) => {
            eprintln!("qualityd: cannot open quality store: {err}");
            return ExitCode::FAILURE;
        }
    };
    let live = Arc::new(LiveService::new(&repo));
    let service: Arc<dyn QualityService> = live.clone();
    let mut studio = Studio::new(service, store);
    if let Some(change) = flag(&args, "--recovery-change") {
        let base = flag(&args, "--base").unwrap_or_else(|| "HEAD".into());
        let head = flag(&args, "--head").unwrap_or_else(|| "WORKTREE".into());
        let desk = match live.recovery_desk(&change, &base, &head) {
            Ok(desk) => desk,
            Err(err) => {
                eprintln!("qualityd: cannot prepare recovery desk: {err}");
                return ExitCode::FAILURE;
            }
        };
        studio = studio.with_recovery(Arc::new(Mutex::new(desk)));
    }
    if let Some(change) = flag(&args, "--protection-change") {
        let base = flag(&args, "--base").unwrap_or_else(|| "HEAD".into());
        let head = flag(&args, "--head").unwrap_or_else(|| "WORKTREE".into());
        let view = match live.protection_view(&change, &base, &head) {
            Ok(view) => view,
            Err(err) => {
                eprintln!("qualityd: cannot prepare protection view: {err}");
                return ExitCode::FAILURE;
            }
        };
        studio = studio.with_protection(Arc::new(Mutex::new(view)));
    }

    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("qualityd: cannot bind {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("qualityd listening on http://{addr} (Quality Studio cockpit at /)");
    match serve(&listener, None, |request| studio.handle(request)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("qualityd: {err}");
            ExitCode::FAILURE
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    let index = args.iter().position(|item| item == name)?;
    args.get(index + 1).cloned()
}
