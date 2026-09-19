//! Detect a Rust coverage producer without inventing a replacement runtime.

use std::process::Command;

/// Which frozen cargo-family executor discovery should insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustCoverageTool {
    /// `cargo llvm-cov` is installed and this target can load `profiler_builtins`.
    LlvmCov,
    /// `cargo tarpaulin` is installed.
    Tarpaulin,
    /// Plain `cargo test`. Measured coverage stays absent.
    None,
}

impl RustCoverageTool {
    /// Probe `cargo --list`. `llvm-cov` is skipped on Windows GNU: rustc
    /// cannot link `profiler_builtins` there, and WVQ does not replace that
    /// runtime.
    #[must_use]
    pub fn detect() -> Self {
        if !cfg!(all(windows, target_env = "gnu")) && cargo_has_subcommand("llvm-cov") {
            Self::LlvmCov
        } else if cargo_has_subcommand("tarpaulin") {
            Self::Tarpaulin
        } else {
            Self::None
        }
    }

    /// Frozen registry id for this producer.
    #[must_use]
    pub fn executor_id(self) -> &'static str {
        match self {
            Self::LlvmCov => "cargo-llvm-cov",
            Self::Tarpaulin => "cargo-tarpaulin",
            Self::None => "cargo-test",
        }
    }
}

fn cargo_has_subcommand(name: &str) -> bool {
    let Ok(output) = Command::new("cargo").arg("--list").output() else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.split_whitespace().next() == Some(name))
}
