# Shadow benchmark methodology

WVQ does not publish a “10×” product claim from this harness.

The harness compares **selected** protection (greedy weighted set cover over
impacted obligations) with the **full** existing suite. It records quality and
cost. It does not invent human-touch-time.

## What is measured

| Metric | Source |
| --- | --- |
| selected vs full test count | `SelectionPlan.selected` vs candidate set |
| selected vs full wall-clock | sum of candidate costs (ms or runner-reported) |
| human bugs/failures recovered | labelled bug recovered iff its protecting test is selected |
| false-positive findings | observed ∧ ¬expected |
| false-negative findings | expected ∧ ¬observed |
| AI tokens | `planning_tokens`, `runtime_tokens` (green path = 0) |
| artifact bytes | CAS size, not LLM context dumps |

Human effort (`human_QA_minutes/PR`, triage, retest) is **optional input**.
It is not estimated from test counts.

## Ecosystems (Task 17)

| Case | Ecosystem | Seed fixtures |
| --- | --- | --- |
| `ts-frontend-sankey` | TS frontend | `fixtures/ts-vitest/` |
| `bun-backend-add` | Node/Bun backend | `fixtures/bun/` |
| `go-service-add` | Go service | `fixtures/go/` |

Synthetic candidate matrices sit on top of those fixtures so selection has
something to cover before a real repo is wired.

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
```
