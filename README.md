# Weavatrix Quality

[![CI](https://github.com/sergii-ziborov/weavatrix-quality/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-quality/actions/workflows/ci.yml)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Turn product intent and repository change into revision-bound proof — without spending LLM tokens on the green path.**

Weavatrix Quality (WVQ) is a Rust-first Spec-to-Proof quality platform. It compiles OpenSpec intent into sealed test obligations, uses `weavatrix-rust` as the only code-intelligence engine, runs the smallest relevant existing test set, and stores immutable `Proof` records.

It is **not** another Playwright wrapper, test runner, browser MCP, coverage dashboard, or click-through LLM agent.

```text
OpenSpec says what should remain true.
Weavatrix says what changed and what it can affect.
Existing runners execute the smallest relevant protection set.
WVQ proves whether old protection survived and new behavior gained proof.
Humans review only unresolved product intent.
```

## Status

**M4.** Domain, OpenSpec, Weavatrix embed, debt ratchet, selection, ledger, Proof, CLI, and bounded MCP are in tree. Read [`docs/STATUS.md`](docs/STATUS.md) before writing code.

Normative specification: [`docs/CANONICAL-MASTER-SPEC.md`](docs/CANONICAL-MASTER-SPEC.md) (2026-08-18).

## Place in the ecosystem

```text
Weavatrix          UNDERSTAND   what exists in source
Weavatrix Quality  PROVE        what must still be true after this change
Weavatrix Loom     COMPOSE      capabilities → ordinary Rust
Cortex Loom        optional     agent context / model routing
```

| Product | Owns | Does **not** own |
| --- | --- | --- |
| **Weavatrix** | Revision-bound code graph | Quality policy, proofs, oracles |
| **Weavatrix Quality** (this) | Spec ↔ code ↔ behavior proof, debt ratchet, protection continuity | A second code graph, OpenSpec authoring, a browser |
| **Weavatrix Loom** | Capability composition | Code intelligence or QA execution |
| **Cortex Loom** | Agent token economy | Anything required for WVQ CI |

WVQ CI must work without Cortex.

## v1 loop (definition of done)

A real TS/JS/Bun/Go repository can:

1. parse OpenSpec + `quality.yaml`
2. seal obligations
3. compare base/head Weavatrix evidence
4. run the Quality Debt Ratchet
5. select a minimal impacted test subset
6. execute existing Vitest/Jest/Bun/Go/Playwright tests
7. normalize JUnit/JSON/LCOV/Go coverage
8. create revision-bound Proofs
9. expose `quality_verify` over CLI/MCP
10. separate `new / existing / fixed / returned` debt
11. consume **zero** LLM runtime tokens on a normal green PR

Recorder, explorer, and mutation come after that loop is real.

## Surfaces

```text
wvq init | spec validate | spec seal | analyze | debt | select | run | verify | explain | record | replay | baseline | doctor
```

MCP default profile (via `mcport`): `quality_context`, `quality_plan`, `quality_run`, `quality_status`, `quality_verify`, `quality_explain`, `quality_evidence`.

No arbitrary shell over MCP. Large artifacts stay behind handles.

## Build

Requires Rust 1.89+.

```sh
cargo test --workspace
```

## License

MIT. See [LICENSE](LICENSE).
