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

**All 35 planned tasks are in tree.** Domain, OpenSpec reader, `OracleSeal`, Weavatrix embed, debt ratchet, the check families, runner normalization, selection, ledger, Proof, CLI, bounded MCP, browser `TestProgram`, `BehaviorGraph`, Delta Triangle, flake triage and safe healing, mutation/metamorphic/explorer, AI Cost Firewall, Quality Studio, spec recovery, and protection continuity.

Read [`docs/STATUS.md`](docs/STATUS.md) before writing code.

Normative specification: [`docs/CANONICAL-MASTER-SPEC.md`](docs/CANONICAL-MASTER-SPEC.md) (2026-08-18).

What is **not** done: the producers. Nothing yet feeds real `graph_diff` output into the impact union, real coverage into `FlowProtection`, or a real model into the AI budget. The rules are built and tested; wiring them to a live repository is the next layer, and the CI rollout in §59 has not started.

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

MCP default profile (via `mcport`) is **seven tools and stays seven**: `quality_context`, `quality_plan`, `quality_run`, `quality_status`, `quality_verify`, `quality_explain`, `quality_evidence`.

Two opt-in profiles sit beside it, off the coding-agent surface so its schema footprint stays small:

```text
spec recovery   quality_spec_recover | _review | _questions | _preview_patch | _verify | _seal
protection      quality_protection   | quality_test_lineage | quality_flow
```

`qualityd` serves the exception-first Studio API over the same command bus. The change dashboard lists only unresolved proofs and counts the green ones; drill-down screens show everything.

No arbitrary shell over MCP. Large artifacts stay behind handles.

## Finding possible defects, without guessing

Routing a change to the right human is necessary but not sufficient. WVQ also turns the *shape* of a change into falsifiable questions with concrete probes — a flipped default asks about the absent value, a retired persisted key asks what happens to records that still carry it, a new membership guard enumerates what falls outside the set.

Two things are tracked separately, and this matters:

- **weight** — how much it would cost if the answer is bad;
- **confidence** — whether the graph actually corroborated the signal, or a regex merely matched some text.

Only `High` weight **and** `Confirmed` confidence may fail a build. This is not caution for its own sake. A shadow run over sixty accepted, defect-free changes had text-matching detectors firing on 42% of them, because words like "viewer" and operators like `<` are everywhere; the two graph-backed detectors fired on 5–8%. A gate that stops two changes in five is a gate people switch off.

Spec §59 Stage C is the rule: a category is promoted to blocking only after its precision is measured on that repository.

## Build

Requires Rust 1.89+.

```sh
cargo test --workspace
```

## License

MIT. See [LICENSE](LICENSE).
