# Architecture (pocket map)

Full design: `CANONICAL-MASTER-SPEC.md` §§1–35, 64, 91–93.

## One sentence

WVQ compiles sealed intent, diffs two Weavatrix revisions, runs the smallest existing protection set, and emits a revision-bound Proof.

## Pipeline

```text
OpenSpec + quality.yaml
        ↓
QualityContract + OracleSeal
        ↓
IntentGraph
        ↓
weavatrix-rust CodeGraph (base ∪ head ∪ removed)
        ↓
Quality Debt Ratchet + RiskEvidence
        ↓
TestProgram / registered existing runners
        ↓
Observations → BehaviorGraph → BehaviorDelta
        ↓
Delta Triangle (Spec / Code / Behavior)
        ↓
Evidence Ledger + CAS
        ↓
Proof → QualityVerdict
        ↓
CLI / MCP / exception-only Studio
```

## Three graphs

- **IntentGraph** — Change, Requirement, Scenario, RiskEvidence, TestObligation, OracleSeal
- **CodeGraph** — owned exclusively by `weavatrix-rust`; WVQ stores refs, not a copy
- **BehaviorGraph** — normalized runtime state and semantic actions

## Delta Triangle

| Spec | Code | Behavior | Reading |
| --- | --- | --- | --- |
| yes | yes | yes | expected change candidate |
| no | yes | yes | unintended behavior drift |
| yes | yes | no | incomplete implementation |
| yes | no | no | requirement with no implementation evidence |
| no | yes | no | probable internal refactor |
| no | no | yes | environment / nondeterminism |
| yes | no | yes | config/external path or stale code evidence |

This table is evidence, not a one-axis verdict.

## Surfaces share one bus

```text
CLI ─┐
HTTP ├→ wvq-command-bus → domain services
MCP ─┘
```

## Storage

```text
.weavatrix-quality/
  quality.db
  objects/ab/abcdef...
```

SQLite + content-addressed artifacts. Proofs are immutable.

## Target repo layout (as crates land)

See spec §34. Workspace members are added only when the matching task starts.
