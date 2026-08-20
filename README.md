# Weavatrix Quality

[![CI](https://github.com/sergii-ziborov/weavatrix-quality/actions/workflows/ci.yml/badge.svg)](https://github.com/sergii-ziborov/weavatrix-quality/actions/workflows/ci.yml)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Turn product intent and a repository change into revision-bound proof — without spending LLM tokens on the green path.**

Weavatrix Quality (WVQ) is a Rust-first Spec-to-Proof quality platform. It compiles OpenSpec intent into sealed test obligations, uses `weavatrix-rust` as its only code-intelligence engine, executes existing registered test runners, and stores immutable evidence and `Proof` records.

```text
OpenSpec says what should remain true.
Weavatrix says what changed and what it can affect.
Existing runners execute the smallest safe protection set.
WVQ proves which obligations have same-revision execution evidence.
Humans review unresolved product intent instead of ordinary green runs.
```

## Status

All 35 tasks in the canonical development plan are implemented. The live vertical is connected end to end:

- repository manifests discover only frozen, bounded executors;
- Weavatrix produces revision-bound `graph_diff`, change impact, test selection, and immutable debt comparison;
- impact is `base ∪ head ∪ removed`, never head-only;
- JUnit, LCOV, `go test -json`, and Go coverprofiles become normalized evidence;
- measured coverage maps onto changed graph nodes and produces a `ProtectionSnapshot`;
- SQLite + CAS preserve runs, evidence, debt history, AI usage, and immutable proofs across processes;
- a passing suite proves only obligations explicitly bound to tests in policy;
- an explicit loopback model call goes through the persistent AI Cost Firewall; normal verification never calls a model.
- agents can author a typed Playwright-backed `TestProgram` from changed-code and sealed-intent context, validate it without writes, preview it through the real browser with screenshot/trace handles, and explicitly promote only that exact passing preview.

Read [`docs/STATUS.md`](docs/STATUS.md) before changing the repository. The normative design is [`docs/CANONICAL-MASTER-SPEC.md`](docs/CANONICAL-MASTER-SPEC.md).

## Place in the ecosystem

```text
Weavatrix          UNDERSTAND   what exists in source
Weavatrix Quality  PROVE        what must still be true after this change
Weavatrix Loom     COMPOSE      capabilities into ordinary Rust
Cortex Loom        optional     agent context and model routing
```

WVQ CI does not depend on Cortex and uses zero runtime LLM tokens on the ordinary green path.

## Live verification loop

A Rust, TS/JS, Bun, Go, or Playwright repository can:

1. parse OpenSpec plus `quality.yaml` and seal obligations;
2. compare base/head Weavatrix graph evidence;
3. classify quality debt as `new / existing / fixed / returned / excepted`;
4. combine static impact with policy-bound obligation coverage;
5. execute Cargo, npm, Vitest, Jest, Bun, Go, or Playwright through registered bounded argv;
6. normalize runner output and fresh coverage artifacts;
7. reject a revision change during execution;
8. assemble and persist same-revision Proofs;
9. expose the result through CLI, MCP, and the `qualityd` Studio service.

An `impacted` run uses a filtered JS/Bun/Playwright subset only when every obligation is covered and every selected path maps safely to a supported filter. Otherwise it widens to `all`; it never labels skipped protection as a successful impacted run. Every run returns `scope_reason`, selected/available test counts, and executor/browser invocation counts. File paths stay file-path argv values rather than becoming test-title regexes, and paths for one runner are batched into bounded processes (at most 128 filters and 24 KiB per process). More than sixteen batches widens to the full suite to avoid process amplification.

Fresh normalized JUnit, Go, and typed browser results also feed persistent test analytics. WVQ records each exact executor/suite/test identity with its revision, outcome, and reported duration; clusters repeated failures by a stable fingerprint; and reports the slowest current cases with their historical mean. A test is called flaky only after the same identity has both passed and failed/errored in recorded history. The bounded `test-analytics` artifact stays in CAS, while `run` returns recorded, failed, flaky, and deterministically unclassified counts. This path makes no model call.

For a package whose test script is exactly `vitest` or `vitest run`, WVQ selects the registered Vitest executor and adds the runner's built-in JUnit reporter automatically. It resolves the repository-local binary through offline, non-interactive npm execution, imports the report, and deletes only WVQ's private `.weavatrix-quality/junit.xml` before checking the revision. Repository-owned report paths are never removed. More complex package scripts stay on the generic npm boundary and can still contribute any fresh supported JUnit/LCOV artifacts they produce.

Measured coverage can teach later selections without replacing Weavatrix. WVQ attributes graph nodes to a test only when a successful executor invocation ran exactly one test path; aggregate coverage from multi-test batches is never guessed onto each member. A historical test enters the base/head candidate union only after the same test-node relation was observed in two distinct runs. The bounded `selection-decision` artifact shows every chosen path, its evidence chain, the observation floor, and uncovered obligations.

`wvq-bench` is also a defensive learning run, not just a timer. After the impacted and full scopes finish at the same change/revision, WVQ compares normalized failing test identities. The audit is `corroborated`, `contradicted`, `unmeasured`, or `not_reduced`; a failure found only by the full run is persisted as a `selection-audit` artifact and its safely resolved test path is fed into future selection for that impacted graph surface. Replaying the same run pair is idempotent, and bounded samples never hide the total miss count.

## CLI

```text
wvq spec validate [--change ID]
wvq spec seal [--change ID]
wvq analyze [--change ID] [--purpose spec|implementation|review] [--token-budget N]
wvq debt [--change ID]
wvq select [--change ID]
wvq run [--change ID] [--scope impacted|all] [--evidence-policy standard|minimal|none]
wvq status
wvq verify [--change ID]
wvq explain <id>
wvq plan [--change ID]
wvq model [--change ID] --kind planning|runtime|browser_escape|vision --prompt TEXT
```

Unknown and duplicate flags fail instead of being silently ignored. A blocking `CONTRADICTED` verify verdict exits with code 2; unresolved evidence exits with code 1.

For an actual selected-vs-full shadow measurement, use the live benchmark binary. It executes both scopes through `LiveService`, records real elapsed time and evidence bytes, and reports when impacted selection widened:

```sh
cargo run -p wvq-bench -- \
  --repo /path/to/repository \
  --change current \
  --base origin/main \
  --head WORKTREE \
  --evidence-policy minimal
```

The older labelled ecosystem cases remain available as deterministic selection-quality fixtures; their declared costs are not presented as measured wall-clock time.

## JavaScript and npm

The `wvq` npm package is a typed JavaScript boundary around the same Rust implementation. It does not fork policy, graph, evidence, proof, or budget behavior into TypeScript. Release artifacts carry `wvq`, `wvq-mcp`, and `wvq-bench` for Windows, macOS, and Linux on x64 and arm64.

```sh
npm install --save-dev wvq
npx wvq --repo . plan --change current
npx wvq mcp --repo .
npx wvq bench --repo . --change current --base origin/main --head WORKTREE
```

Applications can use the typed API or the bounded one-call MCP transport:

```js
import { WvqClient } from 'wvq'
import { WvqMcpClient } from 'wvq/mcp'

const quality = new WvqClient({ repo: process.cwd() })
const plan = await quality.plan({ change: 'current' })

const authoring = new WvqMcpClient({
  repo: process.cwd(),
  profile: 'authoring',
  change: 'current',
  base: 'origin/main',
  head: 'WORKTREE',
})
const draft = await authoring.draft()
const validated = await authoring.validate(candidateProgram)
const preview = await authoring.preview(validated.program, { screenshot: true, trace: true })
const promoted = await authoring.promote(preview.preview_id, validated.program)
```

All launchers use direct argv without a shell. The package includes MCP Registry metadata, and tag releases build and smoke-test all six platform variants before npm and MCP publication. Browser preview resolves Playwright from `browser.module_root`; install Playwright and the required browser engines in the repository that WVQ verifies.

## Repository policy

`.weavatrix-quality/config.yaml` binds concrete tests to compiled obligation IDs. A green runner without this mapping remains `UNPROVEN`.

```yaml
quality_policy_v: 1

test_bindings:
  - path: tests/permissions.spec.ts
    obligations: [permissions-delete]
    cost: 10
    flake_penalty: 0

ratchet:
  exceptions:
    - fingerprint: accepted-legacy-finding
      reason: tracked migration debt
      expires: 2026-12-31

ai:
  endpoint: http://127.0.0.1:11434/v1/chat/completions
  model: local-quality-model
  max_output_tokens: 512
  max_tokens_per_change: 20000
  max_runtime_tokens: 0
  max_browser_escape_calls: 2
  max_vision_calls: 1
  max_cost_micros: 0
  input_micros_per_million: 0
  output_micros_per_million: 0

browser:
  base_url: http://127.0.0.1:3000
  engine: chromium # chromium, firefox, or webkit through Playwright
  headless: true
  timeout_ms: 30000
  module_root: .
  programs: # optional while an agent is drafting its first TestProgram
    - .weavatrix-quality/programs/permissions.json
```

The model endpoint must resolve to loopback and return OpenAI-compatible completion content plus usage counters. WVQ checks the worst-case reservation before connecting, caps the response at 1 MiB, supports content-length and chunked responses, then persists measured usage. Change-local `quality.yaml` AI hints can only reduce the global token ceilings.

## MCP and Studio

The default `mcport` profile exposes exactly seven strict tools:

```text
quality_context  quality_plan  quality_run  quality_status
quality_verify   quality_explain  quality_evidence
```

It has controlled concurrency, cooperative cancellation, no arbitrary shell, and handle-only delivery for large artifacts. Recovery, protection, and authoring servers are opt-in profiles, so the default coding-agent schema footprint remains seven tools.

The authoring profile is fixed at startup to one change and Git range:

```sh
wvq-mcp --repo . --profile authoring --change current --base HEAD --head WORKTREE
# npm distribution:
npx wvq mcp --repo . --profile authoring --change current --base HEAD --head WORKTREE
```

It exposes four high-level operations:

```text
quality_test_draft     complete sealed obligations + bounded changed-code/Weavatrix context
quality_test_validate  strict TestProgram validation against the existing OracleSeal; no writes
quality_test_preview   real Playwright run with observation/screenshot/trace handles; no test save
quality_test_promote   persist only the exact passing same-revision preview under the existing seal
```

`quality_test_draft` normally uses zero model tokens and lets the calling agent produce the candidate. `use_model: true` explicitly requests one configured loopback planning call through the same persistent AI Cost Firewall. Candidate JSON cannot contain or replace oracle predicates, cannot use XPath, shell, JavaScript evaluation, filesystem writes, or unregistered cross-origin API operations. Screenshot capture defaults on for preview; trace capture is opt-in. Promotion revalidates the current program, change, repository revision, and `OracleSeal`, is idempotent for the same preview, and stores canonical JSON in CAS as program revision 1. Later `select` and `run` load the latest matching sealed revision automatically; a stale seal is never executed.

`qualityd` serves the exception-first Studio API over the same command bus. The corresponding authoring endpoints are `POST /api/v1/authoring/draft`, `/validate`, `/preview`, and `/promote`. Its dashboard hides ordinary passing proof noise while drill-down keeps the full evidence trail.

## Defect hypotheses

WVQ keeps two detector properties separate:

- **weight** — the consequence of a bad answer;
- **confidence** — whether Weavatrix named the concrete symbol or a regex merely matched text.

Only `High` weight plus `Confirmed` confidence may block. `corroborate(signal, &GraphFacts)` promotes only the individual signal whose permission symbol, limit symbol, persisted key, or domain is graph-confirmed. `TestMovedWithImplementation` is never promoted by the graph.

In a sixty-change accepted-change shadow corpus, the first text-heavy tuning would have blocked 42% of clean changes; graph-corroborated categories fired on 5–8%. Promotion to a blocking category therefore requires measured repository precision.

## Build

Requires Rust 1.89+.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

CI runs both commands on pushes to `main` and pull requests.

## License

MIT. See [LICENSE](LICENSE).
