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
| M2 | Weavatrix embed + Quality Debt Ratchet + first gates | 4–9 | not started |
| M3 | Runner normalize + coverage map + minimal selection | 10–13 | not started |
| M4 | Store + Proof + CLI/MCP | 14–16 | not started |
| M0b | Shadow benchmark harness | 17 | after M4 (spec listed M0 early; implement when there is something to measure) |
| M5 | Browser TestProgram bridge | 18 | not started |
| M6 | Recorder + BehaviorGraph + base/head diff | 19–20 | not started |
| M7 | Flake + safe healing | 21 | not started |
| M8 | Mutation + metamorphic + cheap explorer | 22 | not started |
| M9 | AI Cost Firewall | 23 (firewall half) | not started |
| M10 | Quality Studio | 23 (studio half) | not started |
| M11 | Spec Recovery + Protection Continuity | 24–35 | not started |
| later | Figma / advanced visual / cross-repo | spec M11 extras | not started |

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

- [ ] **Task 4** — embed `weavatrix-rust` → `feat(intelligence): embed weavatrix engine` — §39
- [ ] **Task 5** — Quality Debt Ratchet → `feat(quality): add no-new-debt ratchet` — §40
- [ ] **Task 6** — architecture + size gates → `feat(checks): gate architecture and size regressions` — §41
- [ ] **Task 7** — dead-code + clone delta → `feat(checks): detect new dead code and clones` — §42
- [ ] **Task 8** — topology drift → `feat(checks): report graph topology drift` — §43
- [ ] **Task 9** — API + history risk → `feat(checks): connect contracts and historical risk` — §44

### M3 — execute less, prove more

- [ ] **Task 10** — runner result normalize → `feat(runtime): normalize test evidence` — §45
- [ ] **Task 11** — bounded executor registry → `feat(runtime): add registered bounded executors` — §46
- [ ] **Task 12** — dynamic coverage ↔ Weavatrix → `feat(coverage): map runtime evidence to impacted code` — §47
- [ ] **Task 13** — minimal selection → `feat(selection): choose minimal impacted regression` — §48

### M4 — product surface

- [ ] **Task 14** — SQLite + CAS → `feat(store): add evidence ledger` — §49
- [ ] **Task 15** — Proof engine → `feat(proof): assemble revision-bound verdicts` — §50
- [ ] **Task 16** — command bus, CLI, MCP → `feat(product): add CLI and bounded MCP` — §51

### Measurement

- [ ] **Task 17** — shadow benchmark harness → `bench: add shadow quality evaluation` — §52

### M5–M10 — behavior and humans

- [ ] **Task 18** — Browser TestProgram vertical slice → `feat(browser): add deterministic TestProgram execution` — §53
- [ ] **Task 19** — record/replay + BehaviorGraph → `feat(behavior): turn manual QA into replayable knowledge` — §54
- [ ] **Task 20** — Delta Triangle → `feat(diff): add Delta Triangle verification` — §55
- [ ] **Task 21** — flake + safe healing → `feat(triage): diagnose and safely heal tests` — §56
- [ ] **Task 22** — mutation, metamorphic, explorer → `feat(advanced): add proof strength and cheap exploration` — §57
- [ ] **Task 23** — AI Cost Firewall + Quality Studio → `feat(studio): add exception-only QA cockpit` — §58

### M11 — brownfield and protection

- [ ] **Task 24** — recovery evidence model → `feat(spec-recovery): model intent evidence` — §89
- [ ] **Task 25** — PR/commit clustering → `feat(spec-recovery): cluster implementation into capability changes`
- [ ] **Task 26** — candidate verifier → `feat(spec-recovery): verify candidate acceptance criteria`
- [ ] **Task 27** — QA review state machine → `feat(spec-recovery): require QA verification`
- [ ] **Task 28** — spec-recovery MCP + Studio → `feat(spec-recovery): add reviewed recovery workflow`
- [ ] **Task 29** — dual-revision impacted surface → `feat(impact): preserve base and head affected flows` — §90
- [ ] **Task 30** — test lineage → `feat(protection): track test lineage across revisions`
- [ ] **Task 31** — ProtectionSnapshot → `feat(protection): snapshot runtime protection by revision`
- [ ] **Task 32** — ProtectionDelta → `feat(protection): compare base and head safety nets`
- [ ] **Task 33** — flow-aware selection → `feat(selection): preserve historical regression protection`
- [ ] **Task 34** — WVQ-PROTECT-001…012 → `feat(checks): gate protection continuity regressions`
- [ ] **Task 35** — protection MCP + UI → `feat(studio): explain test protection continuity`

## Priority reminder (spec §60)

P0: debt ratchet, minimal regression, OpenSpec + OracleSeal, Proof ledger, result/coverage normalize, MCP/CLI.

P1: record/replay, base/head behavior diff, flake triage, safe healing, Studio.

P2: mutation, metamorphic, cheap explorer.

P3+: Figma / vision-heavy exploration.
