# Changelog

## 0.1.0-alpha.1 — 2026-09-06

First public alpha of Weavatrix Quality (WVQ).

### Promise

On a supported web/backend repository, WVQ selects a safe change scope, runs
existing tests and required UI checks under a frozen plan, respects cancel on UI
subpaths, emits a `runtime-profile` artifact, and returns a reproducible report
whose Studio debt range matches the verified run. See
[ADR 0003](docs/adr/0003-alpha-orchestration-gate.md).

### Highlights

- Frozen head-selected browser programs for responsive/base UI measurement (no
  silent full-catalog expansion) with shared run cancel.
- Studio summary debt uses the same revision range as `verify`.
- Surface Evidence Matrix is built once per run finish and reused for the
  cheapest-evidence plan.
- `runtime-profile` CAS artifact records orchestration multiplicities; unknown
  meters are JSON `null`, not fake zeros.
- Linux CI positive self-dogfood: `wvq run` then `wvq verify` for
  `wvq-invariants` (observe-only remains the negative smoke).
- MCP via existing `wvq-mcp` (mcport). Default tools unchanged.

### Distribution

- **npm** package `@weavatrix/wvq@0.1.0-alpha.1` (primary): CLI, MCP, bench natives.
  Unscoped `wvq` is reserved by npm similarity rules; the scoped name is required.
- **crates.io**: publishable workspace closure (libraries + `wvq-cli` / `wvq-mcp` /
  `wvq-bench` / `qualityd`) marked unstable alpha. Primary install path remains
  npm `@weavatrix/wvq`.
- **MCP Registry**: `io.github.Weavatrix/weavatrix-quality`.
- **qualityd** is published so `wvq-cli` integration tests can resolve the
  Studio crate on crates.io; it is still not part of the npm release artifact.

### Not in alpha

Full `RunSnapshot`, `AnalysisSession`, browser process pooling, projection
authority cleanup, bounded parallel scheduling, journal incremental index,
Coverage Autopilot, mobile/cloud. Tracked as the finite post-alpha v1 sequence
in ADR 0003.
