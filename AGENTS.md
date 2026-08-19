# Agent operating rules — Weavatrix Quality

Read this file first. Then read `docs/STATUS.md`. Load only the spec sections named there.

## Product

- **Name:** Weavatrix Quality
- **Repo / crate family:** `weavatrix-quality`
- **Short:** WVQ
- **Binary / CLI:** `wvq`
- **Config dir:** `.weavatrix-quality/`
- **Policy file:** `.weavatrix-quality/config.yaml`

Do not invent other product names (`weavatrix-qa`, `wv-quality`, `qualityrix`, etc.).

## Authority

| Authority | Owns |
| --- | --- |
| `docs/CANONICAL-MASTER-SPEC.md` | Product and architecture. Later, stricter section wins. |
| `docs/STATUS.md` | Current milestone, next task, last commit, open questions. **Update it every session.** |
| `docs/development-plan.md` | Task order. Do not skip ahead. |
| `docs/invariants.md` | Hard rules that must never be “simplified away”. |
| `docs/adr/` | Local decisions that refine or defer the spec. |

If an older idea conflicts with a later spec section, the later stricter decision wins.

## How to start a session

1. Read `docs/STATUS.md`.
2. Read only the spec sections listed under **Load next**.
3. Implement **one** independently reviewable task from `docs/development-plan.md`.
4. TDD: failing test → implementation → `cargo test` for the touched crate.
5. Commit with the message prescribed by the task.
6. Update `docs/STATUS.md` (task status, last commit, next load set).
7. Stop. Do not start the next task unless the user asked for more than one.

## How to resume after compaction or a new agent

Do **not** re-read the 4k-line spec. Trust STATUS + the listed sections.

If STATUS and the working tree disagree, the working tree + `git log` win; fix STATUS.

## Hard constraints

- Rust owns policy, deltas, risk, selection, evidence, proof, storage, budgets, MCP/HTTP semantics.
- `weavatrix-rust` is the only code-intelligence engine. No second parser or code graph.
- OpenSpec is the intent authority. WVQ consumes it; it does not fork it.
- Playwright remains the browser engine. Do not build a browser.
- TypeScript is allowed only on the runtime boundary (Playwright bridge, existing runners).
- Normal green-path verification uses **0 runtime LLM tokens**.
- No arbitrary shell over MCP.
- Large artifacts return as handles.
- AI repair may never silently change a sealed business expectation.
- Missing evidence is never evidence of absence.
- Fail closed on unknown schema versions, unknown actions, invalid evidence, ambiguous revision identity, and attempts to mutate a sealed oracle.
- Impact is **never** computed on head only. Always `Impact(base) ∪ Impact(head) ∪ removed`.
- Implementation evidence can propose intent; it cannot establish intent. Recovered requirements cannot seal without QA verification.
- Global coverage improvement must never hide a local protection loss.

## Crate map (grow as tasks land)

| Crate | Role | First task |
| --- | --- | --- |
| `wvq-domain` | IDs, findings, shared enums | Task 1 |
| `wvq-spec` | OpenSpec + `quality.yaml` + OracleSeal | Task 2 |
| `wvq-intelligence` | Weavatrix embed, debt, checks, selection | Task 4 |
| `wvq-runtime` | Executor registry, JUnit/LCOV/Go normalize | Task 10 |
| `wvq-store` | SQLite + CAS | Task 14 |
| `wvq-proof` | Proof assembly, protection snapshots | Task 15 |
| `wvq-command-bus` | Shared CLI/HTTP/MCP commands | Task 16 |
| `wvq-bench` | Shadow selected-vs-full evaluation | Task 17 |
| `wvq-spec-recovery` | Brownfield spec recovery | Task 24 |
| `apps/wvq-cli` | `wvq` binary | Task 16 |
| `apps/wvq-mcp` | `mcport` MCP | Task 16 |
| `apps/qualityd` | HTTP for Studio | Task 23 |
| `js/playwright-runner` | Thin TS bridge, no AI | Task 18 |

Add a crate to `Cargo.toml` members only when its first task starts.

## TDD and commits

Follow the spec task checklists. Each independently reviewable task ends with one commit using the specified message. Do not mix tasks in one commit.

## Non-goals (do not build)

Rust browser, Playwright replacement, generic shell MCP, OpenSpec fork, multi-agent framework, vision-first browser agent, automatic business-oracle healing, custom JS/Go test runner, Neo4j, one global quality %, full Cartesian matrix, whole-repo mutation on every PR.

## When stuck

Write an ADR under `docs/adr/` rather than silently diverging. Record the open question in `docs/STATUS.md`. Ask the user before changing an invariant.
