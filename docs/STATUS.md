# STATUS — Weavatrix Quality

Last updated: 2026-08-19
Session: sequential implementation (Task 2 done)

## Now

**Milestone:** M1 in progress (domain + OpenSpec reader).

**Current task:** none in progress.

**Next task:** Task 3 — quality contract and OracleSeal (`docs/development-plan.md`, spec §38).

**Load next** (do not load the whole spec):

- spec §6 `quality.yaml`, §7 TestObligation, §8 OracleSeal, §38 Task 3
- `crates/wvq-spec/src/openspec.rs`
- `crates/wvq-spec/src/{quality_yaml,obligations,seal}.rs` (to be created)

## Done

- [x] Adopt canonical master spec 2026-08-18 as the in-repo authority
- [x] Name locked: **Weavatrix Quality** / `weavatrix-quality` / WVQ / `wvq`
- [x] Cargo workspace + empty `wvq-domain`
- [x] Agent context files so later sessions do not re-derive the product
- [x] Task 1 — typed IDs, `Severity`, `FindingState`, `SubjectRef`, `QualityFinding`
- [x] Task 2 — `OpenSpec` change-delta reader with file/line provenance

## Not started

Tasks 3–35. See `docs/development-plan.md`.

## Last commit

`feat(spec): read OpenSpec change deltas`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
