# STATUS — Weavatrix Quality

Last updated: 2026-08-18
Session: repo bootstrap

## Now

**Milestone:** M0 scaffold (workspace + spec + agent context). Not yet M1.

**Current task:** none in progress.

**Next task:** Task 1 — workspace and domain contracts (`docs/development-plan.md`, spec §36).

**Load next** (do not load the whole spec):

- `AGENTS.md`
- `docs/invariants.md`
- spec §7 TestObligation, §8 OracleSeal, §27 Proof, §36 Task 1
- `crates/wvq-domain/src/lib.rs`

## Done

- [x] Adopt canonical master spec 2026-08-18 as the in-repo authority
- [x] Name locked: **Weavatrix Quality** / `weavatrix-quality` / WVQ / `wvq`
- [x] Cargo workspace + empty `wvq-domain`
- [x] Agent context files so later sessions do not re-derive the product

## Not started

M1–M11 and Tasks 1–35. See `docs/development-plan.md`.

## Last commit

`chore: bootstrap weavatrix-quality from canonical master spec`

## Open questions

None that block Task 1. Product-level questions stay in the spec; do not invent answers.

## Do not forget

- This is a **separate product** that embeds `weavatrix-rust`. It is not a feature of `weavatrix` or `weavatrix-loom`.
- v1 first-class ecosystems: JS/TS/Node/Bun and Go.
- Proof is the first-class result, not a test file and not a quality %.
- Dual-revision impact and protection continuity are first-class, not later polish.
