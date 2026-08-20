# Shadow benchmark methodology

WVQ does not publish a “10×” product claim from this harness.

There are two deliberately separate modes:

- labelled cases score selection, known-bug recovery, and false positives/negatives using deterministic fixture metadata;
- the live runner executes `impacted` and `all` sequentially through the production command bus and measures actual elapsed time and persisted evidence.

Neither mode invents human-touch-time. Declared candidate costs from labelled cases are never called measured wall-clock time.

## What is measured

| Metric | Source |
| --- | --- |
| selected vs full test count | labelled mode: `SelectionPlan.selected` vs candidate set |
| selected vs full declared cost | labelled mode: candidate cost plus flake penalty |
| selected vs full wall-clock | live mode: elapsed time around real command-bus execution and evidence reads |
| effective scope + reason | live `RunReply.scope` and `RunReply.scope_reason` |
| human bugs/failures recovered | labelled bug recovered iff its protecting test is selected |
| false-positive findings | observed ∧ ¬expected |
| false-negative findings | expected ∧ ¬observed |
| AI tokens | `planning_tokens`, `runtime_tokens` (green path = 0) |
| artifact bytes | bytes reachable through returned evidence handles; CAS content may deduplicate identical blobs |

Human effort (`human_QA_minutes/PR`, triage, retest) is **optional input**.
It is not estimated from test counts.

## Ecosystems (Task 17)

| Case | Ecosystem | Seed fixtures |
| --- | --- | --- |
| `ts-frontend-sankey` | TS frontend | `fixtures/ts-vitest/` |
| `bun-backend-add` | Node/Bun backend | `fixtures/bun/` |
| `go-service-add` | Go service | `fixtures/go/` |

Synthetic candidate matrices sit on top of those fixtures so selection quality remains deterministic. They are not runtime benchmarks.

## Live measurement rules

The live runner:

1. executes `impacted` and then `all` over the same explicit base/head range;
2. uses only repository-discovered registered executors;
3. reads artifact metadata through the public evidence command;
4. records zero runtime LLM tokens;
5. reports fail-closed widening instead of treating it as a speedup;
6. runs sequentially so the two scopes do not compete for machine resources.

Repository paths are canonicalized before subprocess launch, including Windows short paths. A static selection that would spawn more than sixteen separate filtered processes widens to one full-suite process; the reason is returned in the report.

## 10× publication gate

A 10× headline is refused unless **all** of:

1. `human_touch_minutes` is present (measured, not guessed)
2. `baseline_human_touch_minutes` is present for the same class of change
3. `escaped_regressions_delta <= 0`

Even when the gate is open, report summaries must not print “10×”. The ratio
belongs in a later human-reviewed write-up, not in CI output.

Exact ILP selection is out of scope until this harness shows greedy failing
a labelled case.

## How to run

```sh
cargo test -p wvq-bench

cargo run -p wvq-bench -- \
  --repo /path/to/repository \
  --change current \
  --base origin/main \
  --head WORKTREE \
  --evidence-policy minimal
```
