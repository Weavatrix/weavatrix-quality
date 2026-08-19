# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (Task 1 done)

## Now

**Milestone:** M1 in progress (domain landed).

**Current task:** none in progress.

**Next task:** Task 2 — OpenSpec compatibility reader (`docs/development-plan.md`, spec §37).

**Load next** (do not load the whole spec):

- spec §6 OpenSpec integration, §37 Task 2
- `crates/wvq-domain/src/ids.rs` (`ChangeId`, `RequirementId`, `ScenarioId`)
- `crates/wvq-spec/` (to be created)

## Done

- [x] Adopt canonical master spec 2026-08-18 as the in-repo authority
- [x] Name locked: **Weavatrix Quality** / `weavatrix-quality` / WVQ / `wvq`
- [x] Cargo workspace + empty `wvq-domain`
- [x] Agent context files so later sessions do not re-derive the product
- [x] Task 1 — typed IDs, `Severity`, `FindingState`, `SubjectRef`, `QualityFinding`

## Not started

Tasks 2–35. See `docs/development-plan.md`.

## Last commit

`feat(domain): add stable quality contracts`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
