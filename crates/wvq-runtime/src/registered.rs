//! Frozen production registry. Argv is never taken from MCP or repository scripts.

use super::executor::{ExecutorCapabilities, ExecutorId, ExecutorRegistry, ExecutorSpec};
use super::normalize::RuntimeError;

impl ExecutorRegistry {
    /// Vitest / Jest / Bun / Go / Playwright / Cargo, frozen argv prefixes.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::InvalidArg`] if a built-in id is illegal.
    pub fn production() -> Result<Self, RuntimeError> {
        let mut registry = Self::new();
        register_cargo(&mut registry)?;
        register_javascript(&mut registry)?;
        register_go_and_browser(&mut registry)?;
        Ok(registry)
    }
}

fn register_cargo(registry: &mut ExecutorRegistry) -> Result<(), RuntimeError> {
    registry.register(spec(
        "cargo-test",
        "cargo",
        &["test", "--color", "never", "--workspace", "--all-targets"],
        None,
    )?)?;
    registry.register(spec(
        "cargo-llvm-cov",
        "cargo",
        &[
            "llvm-cov",
            "test",
            "--color",
            "never",
            "--workspace",
            "--all-targets",
            "--lcov",
            "--output-path",
            ".weavatrix/coverage/lcov.info",
        ],
        None,
    )?)?;
    registry.register(spec(
        "cargo-tarpaulin",
        "cargo",
        &[
            "tarpaulin",
            "--color",
            "never",
            "--workspace",
            "--out",
            "Lcov",
            "--output-dir",
            ".weavatrix/coverage",
            "--skip-clean",
            "--",
        ],
        None,
    )?)?;
    Ok(())
}

fn register_javascript(registry: &mut ExecutorRegistry) -> Result<(), RuntimeError> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    registry.register(spec("npm-test", npm, &["test", "--"], None)?)?;
    let vitest = vitest_prefix(false, false);
    registry.register(spec("vitest", npm, &vitest, None)?)?;
    let vitest_coverage = vitest_prefix(true, false);
    registry.register(spec("vitest-coverage", npm, &vitest_coverage, None)?)?;
    let storybook = vitest_prefix(false, true);
    registry.register(spec("storybook-vitest", npm, &storybook, None)?)?;
    let storybook_v8 = vitest_prefix(true, true);
    registry.register(spec("storybook-vitest-v8", npm, &storybook_v8, None)?)?;
    registry.register(spec(
        "jest",
        "jest",
        &[
            "--runInBand",
            "--coverage",
            "--coverageReporters=lcov",
            "--coverageDirectory=coverage",
        ],
        Some("--runTestsByPath"),
    )?)?;
    registry.register(spec(
        "bun-test",
        "bun",
        &["test", "--coverage", "--coverage-reporter=lcov"],
        None,
    )?)?;
    Ok(())
}

fn register_go_and_browser(registry: &mut ExecutorRegistry) -> Result<(), RuntimeError> {
    registry.register(spec(
        "go-test",
        "go",
        &[
            "test",
            "-json",
            "-coverprofile=.weavatrix-quality/go-cover.out",
            "./...",
        ],
        Some("-run"),
    )?)?;
    registry.register(spec("playwright", "playwright", &["test"], None)?)?;
    Ok(())
}

fn vitest_prefix(coverage: bool, storybook: bool) -> Vec<&'static str> {
    let mut args = vec!["exec", "--offline", "--yes=false", "--", "vitest", "run"];
    if storybook {
        args.push("--project=storybook");
    }
    if coverage {
        args.push("--coverage");
        args.push("--coverage.reporter=lcov");
    }
    args.push("--reporter=junit");
    args.push("--outputFile=.weavatrix-quality/junit.xml");
    args
}

fn spec(
    id: &str,
    program: &str,
    prefix: &[&str],
    filter_flag: Option<&str>,
) -> Result<ExecutorSpec, RuntimeError> {
    Ok(ExecutorSpec {
        id: ExecutorId::new(id)?,
        program: program.to_owned(),
        prefix: prefix.iter().map(|item| (*item).to_owned()).collect(),
        filter_flag: filter_flag.map(ToOwned::to_owned),
        capabilities: ExecutorCapabilities {
            cases: true,
            coverage: matches!(
                id,
                "vitest-coverage"
                    | "storybook-vitest-v8"
                    | "jest"
                    | "bun-test"
                    | "go-test"
                    | "cargo-llvm-cov"
                    | "cargo-tarpaulin"
            ),
        },
    })
}
