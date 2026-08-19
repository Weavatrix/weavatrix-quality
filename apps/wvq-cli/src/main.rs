//! `wvq` binary.

use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let output = wvq_cli::run(&args);
    let _ = io::stdout().write_all(output.stdout.as_bytes());
    let _ = io::stderr().write_all(output.stderr.as_bytes());
    ExitCode::from(u8::try_from(output.code).unwrap_or(1))
}
