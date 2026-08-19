# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (M1 complete)

## Now

**Milestone:** M1 done. Next is M2.

**Current task:** none in progress.

**Next task:** Task 4 — embed `weavatrix-rust` (`docs/development-plan.md`, spec §39).

**Load next** (do not load the whole spec):

- spec §3 authority, §10 gate catalogue intro, §39 Task 4
- `weavatrix-rust` `CodeEvidenceProvider` sketch
- `crates/wvq-intelligence/` (to be created)

## Done

- [x] Adopt canonical master spec 2026-08-18 as the in-repo authority
- [x] Name locked: **Weavatrix Quality** / `weavatrix-quality` / WVQ / `wvq`
- [x] Cargo workspace + empty `wvq-domain`
- [x] Agent context files so later sessions do not re-derive the product
- [x] Task 1 — typed IDs, `Severity`, `FindingState`, `SubjectRef`, `QualityFinding`
- [x] Task 2 — `OpenSpec` change-delta reader with file/line provenance
- [x] Task 3 — `quality.yaml` compile + `OracleSeal` (AI metadata does not move the seal)

## Not started

Tasks 4–35. See `docs/development-plan.md`.

## Last commit

`feat(spec): compile and seal quality obligations`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
