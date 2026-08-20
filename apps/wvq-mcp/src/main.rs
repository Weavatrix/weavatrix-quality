//! `wvq-mcp` stdio host. Transport is mcport; semantics are the command bus.

use std::env;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::Mutex;

use wvq_command_bus::{LiveService, QualityService};
use wvq_mcp::{
    HostProfile, parse_host_args, protection_server, quality_server, recovery_server,
    runtime_config,
};

fn main() -> ExitCode {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "wvq-mcp — Weavatrix Quality MCP\n\nUsage:\n  wvq-mcp [--repo PATH] [--profile default|recovery|protection] [--change ID] [--base REF] [--head REF|WORKTREE]"
        );
        return ExitCode::SUCCESS;
    }
    let options = match parse_host_args(&args) {
        Ok(options) => options,
        Err(err) => {
            eprintln!("wvq-mcp: {err}");
            return ExitCode::FAILURE;
        }
    };
    let live = LiveService::new(&options.repo);
    let result = match options.profile {
        HostProfile::Default => {
            let service: Arc<dyn QualityService> = Arc::new(live);
            quality_server(&service).serve(runtime_config())
        }
        HostProfile::Recovery => {
            match live.recovery_desk(&options.change, &options.base, &options.head) {
                Ok(desk) => recovery_server(&Arc::new(Mutex::new(desk))).serve(runtime_config()),
                Err(err) => {
                    eprintln!("wvq-mcp: cannot prepare recovery profile: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
        HostProfile::Protection => {
            match live.protection_view(&options.change, &options.base, &options.head) {
                Ok(view) => protection_server(&Arc::new(Mutex::new(view))).serve(runtime_config()),
                Err(err) => {
                    eprintln!("wvq-mcp: cannot prepare protection profile: {err}");
                    return ExitCode::FAILURE;
                }
            }
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("wvq-mcp: {err}");
            ExitCode::FAILURE
        }
    }
}
