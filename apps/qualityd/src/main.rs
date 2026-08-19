//! `qualityd` binary. Serves the local Quality Studio API.

#![forbid(unsafe_code)]

use std::net::TcpListener;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use qualityd::{Studio, serve};
use wvq_command_bus::{LiveService, QualityService};
use wvq_store::Store;

const DEFAULT_ADDR: &str = "127.0.0.1:7777";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|item| item == "--help" || item == "-h") {
        println!(
            "qualityd — Weavatrix Quality Studio\n\nUsage:\n  qualityd [--repo PATH] [--addr HOST:PORT]\n"
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
    let service: Arc<dyn QualityService> = Arc::new(LiveService::new(&repo));
    let studio = Studio::new(service, store);

    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(err) => {
            eprintln!("qualityd: cannot bind {addr}: {err}");
            return ExitCode::FAILURE;
        }
    };
    println!("qualityd listening on http://{addr}");
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
