//! Frozen runner families. Policy still says `cargo-test`; discovery may
//! upgrade the spawn to the coverage producer already on the machine.

/// Cargo test evidence, with or without a measured coverage producer.
#[must_use]
pub fn is_cargo_test_family(id: &str) -> bool {
    matches!(id, "cargo-test" | "cargo-llvm-cov" | "cargo-tarpaulin")
}

/// Vitest / Storybook Vitest, including the coverage-enabled spawn.
#[must_use]
pub fn is_vitest_family(id: &str) -> bool {
    matches!(
        id,
        "vitest" | "vitest-coverage" | "storybook-vitest" | "storybook-vitest-v8"
    )
}

/// A policy binding of `cargo-test` matches any cargo-family executor.
#[must_use]
pub fn rust_runner_matches(binding: Option<&str>, executor: &str) -> bool {
    is_cargo_test_family(executor)
        && matches!(
            binding,
            None | Some("cargo-test" | "cargo-llvm-cov" | "cargo-tarpaulin")
        )
}
