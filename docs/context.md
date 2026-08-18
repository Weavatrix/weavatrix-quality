# How we keep context across sessions

The spec is ~4.5k lines. Re-reading it every turn will lose the plot and burn tokens. Use this protocol.

## Single source of “where we are”

`docs/STATUS.md` is the session handoff.

An agent that starts cold should be useful after reading, in this order:

1. `AGENTS.md` (~2 min)
2. `docs/STATUS.md` (~1 min)
3. `docs/invariants.md` (~3 min)
4. The spec sections listed in STATUS **Load next**

That is enough to implement the current task.

## What lives where

| File | Role | Update when |
| --- | --- | --- |
| `docs/CANONICAL-MASTER-SPEC.md` | Frozen product law (2026-08-18) | Almost never. New ADR if we must diverge. |
| `docs/STATUS.md` | Live cursor | End of every session / every merged task |
| `docs/development-plan.md` | Checkbox plan | When a task flips to done |
| `docs/invariants.md` | Pocket constitution | Only if the spec gains a new hard rule |
| `docs/adr/NNNN-*.md` | Local decisions | When we refine, defer, or interpret the spec |
| `git log` | Ground truth of what landed | Every commit |

## Why this works

- **STATUS is small.** Future-you does not reconstruct milestone state from chat.
- **Spec is section-addressable.** Load §36 for Task 1, not §1–91.
- **One task per commit.** A crashed session resumes from `git log` + STATUS.
- **Invariants are extracted.** Agents cannot “forget” dual-revision impact or the 0-token green path just because they did not reload §68.
- **ADRs absorb drift.** If implementation must defer mutation to M8, write it down instead of silently dropping it.

## Session recipe for implementers

```text
1. Read STATUS. If Next task is Task N, do only Task N.
2. Write the failing test named in the spec.
3. Make it pass. Do not design the next crate.
4. cargo test -p <crate>
5. Commit with the prescribed message.
6. Tick the task in development-plan.md.
7. Rewrite STATUS: next task, Load next, last commit.
```

## Session recipe after a long break

```text
git log --oneline -20
read docs/STATUS.md
if STATUS.next_task crate does not exist → start that crate
if tests fail → fix before advancing
```

## What not to do

- Do not paste the master spec into every prompt.
- Do not implement Studio, mutation, or explorer “while we are here”.
- Do not move implementation into `weavatrix` or `weavatrix-rust`.
- Do not treat a chat summary as more authoritative than STATUS + git.
- Do not claim “10× QA” until the benchmark harness has human-touch data.

## Suggested git shape

```text
main                 shippable milestone slices
task/01-domain       Task 1
task/02-openspec     Task 2
...
```

Merge one task branch at a time. Do not accumulate five unfinished crates on one branch.

## Suggested later attachments

Once the repo has history, attach BranchPilot (session journal) and repo-lens (blast radius) to this repo. They complement STATUS; they do not replace it.
