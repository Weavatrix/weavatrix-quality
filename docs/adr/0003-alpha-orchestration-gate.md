# ADR 0003 — Alpha orchestration gate

Status: accepted
Date: 2026-09-06

## Context

WVQ already has enough mechanisms for a useful first product: OpenSpec/OracleSeal,
Weavatrix embed, selected execution, Playwright TestPrograms, Proof, Protection,
Debt, UI Integrity, MCP (mcport), Studio, and npm packaging. What blocks a coherent
release is not another evidence family. Three systemic issues dominate:

1. Subsystems recompute the same facts and expand execution scope independently
   (notably responsive UI replaying the full browser catalog while the head run
   selected a subset; base/responsive paths minting dead local cancel flags).
2. Report views can disagree on provenance (Studio debt hardcoding `HEAD`/`WORKTREE`
   while verify uses the stored run range).
3. The readiness boundary keeps expanding with every new idea.

A 2026-09-06 architecture review recommended a finite sequence: one plan → one
execution set → one consistent report — without abandoning OpenSpec, Weavatrix,
or UI Integrity.

## Decision

### Alpha vs v1

- **Alpha (`0.1.0-alpha.1`)** ships a bounded orchestration gate plus public
  distribution. It does **not** claim the full v1 Definition of Done.
- **v1** remains the finite post-alpha sequence below. New ideas do not become
  alpha blockers. Changing assurance policy requires its own review.

### Alpha promise

On a supported web/backend project, WVQ selects a safe change scope, runs existing
tests and required UI checks under a frozen plan, respects cancel/budget on UI
subpaths, emits a `runtime-profile` artifact, and returns a reproducible report
whose Studio/CLI debt range matches the verified run. MCP remains the existing
mcport host (`quality_*` tools). Primary install path is npm `wvq`; crates.io
publishes the workspace closure needed by the binaries with an explicit unstable
alpha API warning on libraries.

### Alpha gate (must land before the alpha tag)

1. Frozen selected browser programs + shared cancel through base replay and
   responsive measurement (no silent full-catalog expansion).
2. Studio debt range aligned with the verified run; surface matrix built once
   per run finish and reused for the cheapest-evidence plan.
3. `runtime-profile` CAS artifact (real counts; unknown meters are `null`).
4. CI positive self-dogfood: `wvq run` then `wvq verify` for `wvq-invariants`
   (observe-only remains the negative smoke).
5. User docs, examples, CHANGELOG; version lock across Cargo/npm/`server.json`.
6. Publish: npm + MCP Registry + GitHub Release; crates.io for the publishable
   dependency closure of `wvq` / `wvq-mcp` / `wvq-bench` (and `qualityd`, so
   `wvq-cli` can resolve Studio as a versioned path dep on crates.io).

### Deferred to v1 (not alpha incompleteness)

| # | Change |
| --- | --- |
| 1 | Full request-local `AnalysisSession` (one Weavatrix handle per content revision) |
| 2 | Immutable `RunSnapshot` with strictly read-only report/verify/Studio projections |
| 3 | Browser process reuse with fresh context per program |
| 4 | Projection authority cleanup (Proof from Proof records, measured-scope Absent, real ProducerInventory) |
| 5 | Bounded resource-class scheduling (time-to-first-blocker vs full verdict) |
| 6 | Incremental journal indexing |
| 7 | `wvq check` thin orchestration UX + `pr`/`deep` profiles as first-class plan digests |

Also deferred: Coverage Autopilot closed loop, mobile/device farm, cloud fleet,
full metamorphic adapters, React profiler product, perceptual visual engine.

### Distribution

- **npm `wvq`**: primary product distribution (CLI, MCP, bench natives).
- **crates.io**: publish the library + binary closure required for `cargo install`;
  library crates are marked unstable alpha. Do not treat crates.io as a stable
  embedding API until a later ADR.
- **MCP**: existing `wvq-mcp` via mcport; registered as
  `io.github.Weavatrix/weavatrix-quality`. No second MCP stack.

## Consequences

- Agents implement one independently reviewable commit per alpha gate item.
- STATUS “Load next” after alpha points at the deferred v1 sequence, not new
  feature families.
- Alpha PASS does not mean “full deep audit” or universal autonomous QA.
