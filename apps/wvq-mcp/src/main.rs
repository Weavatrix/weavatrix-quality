//! `wvq-mcp` stdio host. Transport is mcport; semantics are the command bus.

use std::env;
use std::sync::Arc;

use wvq_command_bus::LiveService;
use wvq_mcp::{quality_server, runtime_config};

fn main() -> std::io::Result<()> {
    let repo = env::current_dir()?;
    let service: Arc<dyn wvq_command_bus::QualityService> = Arc::new(LiveService::new(repo));
    quality_server(&service).serve(runtime_config())
}
