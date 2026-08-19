# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (Task 18 done)

## Now

**Milestone:** M5 done (TestProgram + thin Playwright bridge). M6 next.

**Current task:** none in progress.

**Next task:** Task 19 — record/replay + BehaviorGraph (`docs/development-plan.md`, spec §54).

**Load next** (do not load the whole spec):

- spec §54 Task 19, §14 TestProgram, BehaviorGraph
- `crates/wvq-runtime` program + browser_protocol
- `js/playwright-runner` record.ts stub

## Done

- [x] Adopt canonical master spec 2026-08-18 as the in-repo authority
- [x] Name locked: **Weavatrix Quality** / `weavatrix-quality` / WVQ / `wvq`
- [x] Cargo workspace + empty `wvq-domain`
- [x] Agent context files so later sessions do not re-derive the product
- [x] Task 1 — typed IDs, `Severity`, `FindingState`, `SubjectRef`, `QualityFinding`
- [x] Task 2 — `OpenSpec` change-delta reader with file/line provenance
- [x] Task 3 — `quality.yaml` compile + `OracleSeal` (AI metadata does not move the seal)
- [x] Task 4 — embed `weavatrix-rust` as the only `CodeEvidenceProvider` (no second graph)
- [x] Task 5 — Quality Debt Ratchet (`existing/new/fixed/returned/excepted`)
- [x] Task 6 — architecture + size gates (`WVQ-ARCH-*`, `WVQ-SIZE-*`)
- [x] Task 7 — dead-code + clone delta (`WVQ-DEAD-*`, `WVQ-CLONE-*`)
- [x] Task 8 — topology drift (`WVQ-GRAPH-*`, base/head numbers)
- [x] Task 9 — API + history risk (`WVQ-API-*`, `WVQ-HIST-*`, `RiskEvidence[]`)
- [x] Task 10 — runner result normalization (JUnit / LCOV / `go test -json`)
- [x] Task 11 — bounded executor registry (no arbitrary shell)
- [x] Task 12 — dynamic coverage ↔ Weavatrix (`WVQ-COV-*`, unmeasured ≠ uncovered)
- [x] Task 13 — minimal selection (greedy weighted set cover + explanation chain)
- [x] Task 14 — SQLite + CAS evidence ledger (immutable proofs)
- [x] Task 15 — Proof engine (`PROVEN`/`CONTRADICTED`/`PARTIAL`/`UNPROVEN`/`HUMAN_REQUIRED`)
- [x] Task 16 — command bus, CLI `wvq`, MCP via `mcport` (seven default tools)
- [x] Task 17 — shadow benchmark harness (selected vs full; no published 10×)
- [x] Task 18 — Browser `TestProgram` IR + thin Playwright bridge (no AI)

## Not started

Tasks 19–35. See `docs/development-plan.md`.

## Last commit

`feat(browser): add deterministic TestProgram execution`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
