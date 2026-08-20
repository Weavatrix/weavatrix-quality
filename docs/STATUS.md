# STATUS — Weavatrix Quality

Last updated: 2026-08-20
Session: Playwright TestProgram authoring tools

## Now

All 35 tasks in `docs/development-plan.md` are implemented. The previously disconnected production paths are part of `LiveService`. This session adds an opt-in agent authoring path on top of that live vertical.

Release state: the live-producer implementation (`ce2a9d5`) and Linux portability fix (`73edbed`) are published on `main`. GitHub Actions run [32378815119](https://github.com/sergii-ziborov/weavatrix-quality/actions/runs/32378815119) passed the clean-checkout workspace tests and Clippy job.

Local release validation: 313 tests passed with zero failures; workspace Clippy passed for all targets with warnings denied.

Unreleased authoring state: a command-bus draft/validate/preview API, a three-tool `--profile authoring` MCP server, and three `qualityd` POST endpoints are implemented locally. The default MCP profile remains exactly seven tools.

## Playwright authoring path

| Operation | Live behavior |
| --- | --- |
| draft | Resolves an explicit base/head range, requires changed code, queries same-revision `graph_diff` and change impact, returns bounded intent/graph context, and never truncates the sealed obligation authority |
| optional model | `use_model: true` performs one planning call through the existing loopback-only persistent AI Cost Firewall; normal draft and verification use zero model tokens |
| validate | Strictly decodes canonical `TestProgram` JSON, rejects candidate-owned oracle fields, unknown/duplicate obligations, XPath, unknown actions, and obligations without an executable sealed expected predicate |
| preview | Executes actual Playwright (`chromium`, `firefox`, or `webkit`), checks repository revision before/after, imports observations/screenshots/trace into CAS, removes exact temporary evidence files, and never saves or registers the candidate |
| transports | MCP: `quality_test_draft`, `quality_test_validate`, `quality_test_preview`; HTTP: `POST /api/v1/authoring/{draft,validate,preview}` |

Affected-package validation: 107 tests passed with zero failures, including the real Rust → stdio bridge → Playwright preview with two screenshots and a trace. Clippy passed for `wvq-runtime`, `wvq-command-bus`, `wvq-mcp`, and `qualityd`, all targets, with warnings denied.

## Live production path

| Producer | Live behavior |
| --- | --- |
| execution | Discovers Cargo/npm/Vitest/Jest/Bun/Go/Playwright manifests and invokes only frozen bounded executor definitions |
| selection | Combines Weavatrix head impact, base-only removed test evidence, and explicit obligation bindings; incomplete/unsafe filters widen to the full suite |
| graph impact | Persists `graph_diff`, change impact, static selection, and `Impact(base) ∪ Impact(head) ∪ removed` at one exact revision |
| coverage | Normalizes fresh LCOV/Go evidence, maps it to changed graph nodes, and persists a revision-bound `ProtectionSnapshot` |
| evidence | Stores run/items, raw streams by policy, normalized results, semantic maps, summaries, and large blobs through SQLite + CAS |
| proof | Uses only the latest same-change, same-revision run; a green suite proves only obligations explicitly bound to executed tests |
| debt | Uses immutable base/head Weavatrix evidence and persistent fixed-history to classify `new/existing/fixed/returned/excepted` |
| AI | Explicit opt-in loopback completion path, preflight reservation, server usage evidence, global + change-local ceilings, persistent per-change spend |

`plan` reads existing same-revision proofs. `explain` resolves obligations, proofs, runs, selections, and debt findings with provenance. `status`, evidence handles, proofs, debt history, and AI usage survive a new process.

## Safety invariants exercised

- no arbitrary shell over MCP;
- large artifacts remain handles;
- unknown schema versions, command values, stale/malformed evidence, revision drift, and incomplete graph diffs fail closed;
- missing coverage is unmeasured, never uncovered;
- a successful unbound suite remains `UNPROVEN`;
- normal verification makes no model call and spends zero runtime tokens;
- model calls accept loopback HTTP only and are refused before network I/O when budget cannot cover the reservation;
- detector blocking requires `High` weight and per-signal `Confirmed` graph corroboration.

## Measured detector calibration

On sixty accepted, defect-free changes, text matching fired on 33–92% depending on category and the initial policy would have blocked 42% of clean changes. Graph-backed default-flip and retired-persisted-key categories fired on 5–8%. The graph promotes only the signal whose concrete symbol it names; `TestMovedWithImplementation` is never promoted.

## Repository maintenance debt

- `cargo fmt --all -- --check` has roughly forty pre-existing formatting differences. This session formats only touched files so that unrelated churn is not mixed into the release.
- `[profile.dev] debug = "line-tables-only"` remains in the workspace manifest to keep local build artifacts bounded.

## Load next

After committing the authoring slice, load `docs/benchmark-methodology.md` for the requested real WVQ benchmark and the TypeScript runtime-boundary sections before designing the JS distribution package. Rust remains the authority for policy and proof semantics.
