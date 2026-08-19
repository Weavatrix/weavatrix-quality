# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (Task 8 done)

## Now

**Milestone:** M2 in progress (through topology). Next: API + history risk.

**Current task:** none in progress.

**Next task:** Task 9 — API + history risk (`docs/development-plan.md`, spec §44).

**Load next** (do not load the whole spec):

- spec §10.6 API, §10.8 history, §13 risk, §44 Task 9
- `crates/wvq-intelligence/src/checks/`
- Weavatrix `list_endpoints` / `git_history`

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

## Not started

Tasks 9–35. See `docs/development-plan.md`.

## Last commit

`feat(checks): report graph topology drift`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
