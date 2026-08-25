# Development plan

Source of truth for *what* and *why*: `CANONICAL-MASTER-SPEC.md`.
Source of truth for *where we are*: `STATUS.md`.
This file is the checkbox list.

Rule: one task, one commit, prescribed message. TDD. Do not start Task N+1 until N is merged.

## Milestones

| Milestone | Deliverable | Tasks | Status |
| --- | --- | --- | --- |
| M0 | Shadow-ready workspace, spec, agent context | bootstrap | **done** |
| M1 | Domain + OpenSpec + OracleSeal | 1–3 | **done** |
| M2 | Weavatrix embed + Quality Debt Ratchet + first gates | 4–9 | **done** |
| M3 | Runner normalize + coverage map + minimal selection | 10–13 | **done** |
| M4 | Store + Proof + CLI/MCP | 14–16 | **done** |
| M0b | Shadow benchmark harness | 17 | **done** |
| M5 | Browser TestProgram bridge | 18 | **done** |
| M6 | Recorder + BehaviorGraph + base/head diff | 19–20 | **done** |
| M7 | Flake + safe healing | 21 | **done** |
| M8 | Mutation + metamorphic + cheap explorer | 22 | **done** |
| M9 | AI Cost Firewall | 23 (firewall half) | **done** |
| M10 | Quality Studio | 23 (studio half) | **done** |
| M11 | Spec Recovery + Protection Continuity | 24–35 | **done** |
| later | Figma / advanced visual / cross-repo | spec M11 extras | outside the 35-task plan |
| after M11 | Soundness before new families | P0 a11y/coverage/reach/mutation | **done** (library); P1 wire Surface Graph next |

M11 items are first-class, not polish. Dual-revision impact (Task 29) should inform selection as soon as M3 exists — do not “forget” base-only removals while waiting for Studio.

## CI rollout (after M4)

- **Stage A** — observe only, 30–50 PRs
- **Stage B** — block only objective new debt
- **Stage C** — promote calibrated warnings
- **Stage D** — automatic eligible verdict for low/medium-risk complete Proofs

## Tasks

### M1 — contracts and intent

- [x] **Task 1** — domain contracts → `feat(domain): add stable quality contracts` — spec §36
- [x] **Task 2** — OpenSpec reader → `feat(spec): read OpenSpec change deltas` — §37
- [x] **Task 3** — quality.yaml + OracleSeal → `feat(spec): compile and seal quality obligations` — §38

### M2 — Weavatrix intelligence

- [x] **Task 4** — embed `weavatrix-rust` → `feat(intelligence): embed weavatrix engine` — §39
- [x] **Task 5** — Quality Debt Ratchet → `feat(quality): add no-new-debt ratchet` — §40
- [x] **Task 6** — architecture + size gates → `feat(checks): gate architecture and size regressions` — §41
- [x] **Task 7** — dead-code + clone delta → `feat(checks): detect new dead code and clones` — §42
- [x] **Task 8** — topology drift → `feat(checks): report graph topology drift` — §43
- [x] **Task 9** — API + history risk → `feat(checks): connect contracts and historical risk` — §44

### M3 — execute less, prove more

- [x] **Task 10** — runner result normalize → `feat(runtime): normalize test evidence` — §45
- [x] **Task 11** — bounded executor registry → `feat(runtime): add registered bounded executors` — §46
- [x] **Task 12** — dynamic coverage ↔ Weavatrix → `feat(coverage): map runtime evidence to impacted code` — §47
- [x] **Task 13** — minimal selection → `feat(selection): choose minimal impacted regression` — §48

### M4 — product surface

- [x] **Task 14** — SQLite + CAS → `feat(store): add evidence ledger` — §49
- [x] **Task 15** — Proof engine → `feat(proof): assemble revision-bound verdicts` — §50
- [x] **Task 16** — command bus, CLI, MCP → `feat(product): add CLI and bounded MCP` — §51

### Measurement

- [x] **Task 17** — shadow benchmark harness → `bench: add shadow quality evaluation` — §52

### M5–M10 — behavior and humans

- [x] **Task 18** — Browser TestProgram vertical slice → `feat(browser): add deterministic TestProgram execution` — §53
- [x] **Task 19** — record/replay + BehaviorGraph → `feat(behavior): turn manual QA into replayable knowledge` — §54
- [x] **Task 20** — Delta Triangle → `feat(diff): add Delta Triangle verification` — §55
- [x] **Task 21** — flake + safe healing → `feat(triage): diagnose and safely heal tests` — §56
- [x] **Task 22** — mutation, metamorphic, explorer → `feat(advanced): add proof strength and cheap exploration` — §57
- [x] **Task 23** — AI Cost Firewall + Quality Studio → `feat(studio): add exception-only QA cockpit` — §58

### M11 — brownfield and protection

- [x] **Task 24** — recovery evidence model → `feat(spec-recovery): model intent evidence` — §89
- [x] **Task 25** — PR/commit clustering → `feat(spec-recovery): cluster implementation into capability changes`
- [x] **Task 26** — candidate verifier → `feat(spec-recovery): verify candidate acceptance criteria`
- [x] **Task 27** — QA review state machine → `feat(spec-recovery): require QA verification`
- [x] **Task 28** — spec-recovery MCP + Studio → `feat(spec-recovery): add reviewed recovery workflow`
- [x] **Task 29** — dual-revision impacted surface → `feat(impact): preserve base and head affected flows` — §90
- [x] **Task 30** — test lineage → `feat(protection): track test lineage across revisions`
- [x] **Task 31** — ProtectionSnapshot → `feat(protection): snapshot runtime protection by revision`
- [x] **Task 32** — ProtectionDelta → `feat(protection): compare base and head safety nets`
- [x] **Task 33** — flow-aware selection → `feat(selection): preserve historical regression protection`
- [x] **Task 34** — WVQ-PROTECT-001…012 → `feat(checks): gate protection continuity regressions`
- [x] **Task 35** — protection MCP + UI → `feat(studio): explain test protection continuity`

## Priority reminder (spec §60)

P0: debt ratchet, minimal regression, OpenSpec + OracleSeal, Proof ledger, result/coverage normalize, MCP/CLI.

P1: record/replay, base/head behavior diff, flake triage, safe healing, Studio.

P2: mutation, metamorphic, cheap explorer.

P3+: Figma / vision-heavy exploration.

## After M11 (not a new 35-task list)

The 35 tasks are complete. Remaining work is soundness of what already exists, then wiring, then new families. See `STATUS.md` **Load next**.

P0 soundness (done in library / bridge):

- [x] Playwright bridge manifest is generated from `dist/*.js`, not a manual `BRIDGE_FILES` list
- [x] a11y truncation propagates TypeScript → Rust; axe absent ≠ axe failed
- [x] Coverage Autopilot: one hit is `measured_partial`, not the whole surface; graph `truncated` is copied
- [x] `production_nodes_for_binding` is directed, returns `truncated` + evidence paths, and a truncated walk is not a CodeDelta surface
- [x] Mutation does not fall back `owners.empty → all candidates`

P1 (do not skip into closed-loop Autopilot):

- [x] Wire `ApplicationSurfaceGraph` as a read-only MCP/Studio artifact
- [ ] Surface Evidence Matrix
- [ ] Gap classification + cheapest-evidence planner
- [ ] Observe-only calibration on real PRs

The command-bus 300-line file split is done (`service/` ≤300, Clippy `-D warnings` green). Bench and gap analysis in `wvq-command-bus` are no longer blocked on file size.
