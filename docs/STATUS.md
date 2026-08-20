# STATUS — Weavatrix Quality

Last updated: 2026-08-20
Session: sequential implementation (Tasks 1–35 done, plus the hypothesis engine)

## Now

**All 35 tasks of the canonical plan are implemented, tested and committed.**

**Current task:** none in progress.

**Next:** not more implementation. The plan's own §59 says the next step is a
CI rollout in observe-only mode, and §62 says nothing may be called "10×" until
human-effort data exists. Both are measurement, not code.

## What is built

| Range | Delivers |
| --- | --- |
| Tasks 1–3 | typed domain, `OpenSpec` change-delta reader, `OracleSeal` |
| Tasks 4–9 | Weavatrix embed, Quality Debt Ratchet, `WVQ-ARCH/SIZE/DEAD/CLONE/GRAPH/API/HIST-*` |
| Tasks 10–13 | JUnit/LCOV/`go test -json` normalization, bounded executors, coverage↔graph, minimal selection |
| Tasks 14–17 | SQLite + CAS ledger, Proof engine, CLI + bounded MCP, shadow benchmark harness |
| Tasks 18–21 | browser `TestProgram`, record/replay `BehaviorGraph`, Delta Triangle, flake triage + safe healing |
| Tasks 22–23 | mutation, metamorphic relations, cheap explorer, AI Cost Firewall, Quality Studio |
| Tasks 24–28 | `wvq-spec-recovery`: intent-evidence tiers, capability clustering, candidate verifier, mandatory QA review, recovery MCP + Studio |
| Tasks 29–35 | dual-revision impact, test lineage, `ProtectionSnapshot`/`Delta`, flow-aware selection, `WVQ-PROTECT-001…012`, protection MCP + Studio |
| beyond plan | defect-hypothesis engine with per-signal confidence |

## What is deliberately not built

The **producers**. Nothing yet feeds live `graph_diff` output into the impact
union, real measured coverage into `FlowProtection`, or a real model into the AI
budget. `charge()` and `put_ai_usage` are exercised only by tests, which is
correct at this stage: the ordinary green path spends zero runtime tokens by
design, so the AI callers appear only when the explorer's escape packet and the
flake decision packet are wired to a model.

The rules are built and tested. Connecting them to a live repository is the next
layer of work.

## Measured, not assumed

A shadow run over sixty accepted, defect-free changes in a real repository:

| Detector kind | Fired on | Verdict |
| --- | ---: | --- |
| text-matching (permissions, boundaries, folds, test co-change) | 33–92% | too noisy to gate |
| graph-backed (default flip, retired persisted key) | 5–8% | usable |

42% of clean changes would have been blocked by the first tuning. That is why
`SignalConfidence` exists and why only `High` weight **and** `Confirmed`
confidence may fail a build. Promote a category only after its precision is
measured on the repository in question — spec §59 Stage C.

## Known debt in this repository

- `cargo fmt --all -- --check` fails on roughly forty pre-existing files. The
  drift predates the current work and was deliberately not mixed into feature
  commits.
- `[profile.dev] debug = "line-tables-only"` is set in the workspace manifest.
  It cuts `target/` from about 3 GB to 1.35 GB; revert the line if full
  debuginfo is wanted back.
