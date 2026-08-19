# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (Task 12 done)

## Now

**Milestone:** M3 in progress (through coverage mapping).

**Current task:** none in progress.

**Next task:** Task 13 — minimal selection (`docs/development-plan.md`, spec §48).

**Load next** (do not load the whole spec):

- spec §19 impact-based selection, §48 Task 13
- `crates/wvq-intelligence` coverage + risk
- Weavatrix `select_tests`

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

## Not started

Tasks 13–35. See `docs/development-plan.md`.

## Last commit

`feat(coverage): map runtime evidence to impacted code`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
