# Weavatrix Quality

[![CI](https://github.com/Weavatrix/weavatrix-quality/actions/workflows/ci.yml/badge.svg)](https://github.com/Weavatrix/weavatrix-quality/actions/workflows/ci.yml)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

The Spec-to-Proof quality layer of the [Weavatrix ecosystem](https://weavatrix.com/ecosystem).

**Turn product intent and a repository change into revision-bound proof — without spending LLM tokens on the green path.**

Weavatrix Quality (WVQ) is a Rust-first Spec-to-Proof quality platform. It compiles OpenSpec intent into sealed test obligations, uses `weavatrix-rust` 2.7.4 as its only code-intelligence engine, executes existing registered test runners, and stores immutable evidence and `Proof` records.

```text
OpenSpec says what should remain true.
Weavatrix says what changed and what it can affect.
Existing runners execute the smallest safe protection set.
WVQ proves which obligations have same-revision execution evidence.
Humans review unresolved product intent instead of ordinary green runs.
```

## Status

The canonical development checklist is implemented, but its items have different maturity levels. The core live vertical is connected; the [maturity matrix](docs/STATUS.md#maturity-matrix) distinguishes contracts and library primitives from wired, measured execution:

- repository manifests discover only frozen, bounded executors;
- every Git range resolves requested base SHA, checked-out head SHA, and merge-base; Weavatrix deltas use that common ancestor;
- Weavatrix produces revision-bound `graph_diff`, change impact, test selection, and immutable debt comparison;
- impact is `base ∪ head ∪ removed`, never head-only;
- Cargo/libtest, JUnit, LCOV, `go test -json`, and Go coverprofiles become normalized evidence;
- measured coverage maps onto changed graph symbols and produces base/head `ProtectionSnapshot` evidence for the default verdict axis;
- SQLite + CAS preserve runs, evidence, proof-artifact provenance, debt history, AI usage, and immutable proofs across processes;
- proof requires the exact configured runner case or Playwright assertion, not a green file path;
- unambiguous single-case coverage is attributed to its exact bound test; aggregate coverage stays executor-level instead of being guessed onto cases;
- every passing normalized case is inventoried independently of impacted coverage, so a green test that reaches no relevant symbol is reported as a phantom protector instead of looking deleted;
- a committed React/Node/Go/OpenSpec fixture proves safe relocation, phantom protection, sole-protector deletion, approved business-expectation replacement, and changed-symbol recovery across CLI, MCP, and `qualityd`;
- when an `OracleSeal` changes, WVQ prepares one immutable base/head/merge-base/content-revision review packet; ordinary analysis is automatic, while only an exact digest-matching QA or product-owner acceptance can authorize the new intent;
- an explicit loopback model call goes through the persistent AI Cost Firewall; normal verification never calls a model.
- agents can author a typed Playwright-backed `TestProgram` from changed-code and sealed-intent context, validate it without writes, preview it through the real browser with screenshot/trace handles, and explicitly promote only that exact passing preview.
- every Playwright run turns route, accessibility digest, viewport, semantic action, sealed obligation, and observed API metadata into persistent BehaviorGraph states/edges and a bounded contribution artifact.
- every normal browser run replays the exact same head-selected `TestProgram` on the merge base, compares structured observations before a named visual digest, and feeds the live Spec x Code x Behavior Delta Triangle into `quality_verify` with zero model tokens.
- the spec axis of that triangle is authorized per program against the exact requirements and scenarios that changed between base and head, so editing one requirement never excuses behavior drift in a program bound to a different one.
- mutation-enabled scenarios run concrete changed-line TS/JS or Go source edits in an isolated Git worktree and attach exact per-obligation killed/survived evidence to the ordinary Proof.

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
5. execute Cargo, npm, Vitest, Storybook/Vitest, Jest, Bun, Go, or Playwright through registered bounded argv;
6. normalize runner output and fresh coverage artifacts;
7. reject a revision change during execution;
8. assemble and persist same-revision Proofs;
9. expose the result through CLI, MCP, and the `qualityd` Studio service.

An `impacted` run uses a filtered JS/Bun/Playwright subset only when every obligation is covered and every selected path maps safely to a supported filter. Otherwise it widens to `all`; it never labels skipped protection as a successful impacted run. Every run returns `scope_reason`, selected/available test counts, and executor/browser invocation counts. File paths stay file-path argv values rather than becoming test-title regexes, and paths for one runner are batched into bounded processes (at most 128 filters and 24 KiB per process). More than sixteen batches widens to the full suite to avoid process amplification.

Fresh normalized JUnit, Go, and typed browser results also feed persistent test analytics. WVQ records each exact executor/suite/test identity with its revision, outcome, and reported duration; clusters repeated failures by a stable fingerprint; and reports the slowest current cases with their historical mean. A test is called flaky only after the same identity has both passed and failed/errored in recorded history. The bounded `test-analytics` artifact stays in CAS, while `run` returns recorded, failed, flaky, and deterministically unclassified counts. This path makes no model call.

Playwright observations also feed the live BehaviorGraph. WVQ hashes canonical route + accessibility + viewport state, persists semantic transitions between adjacent observations, links them to the program's sealed obligations and observed API operations, and reports both run-local and newly learned state/edge counts. The bounded `behavior-contribution` artifact records exact run/revision provenance and zero runtime LLM tokens. Browser coverage stays explicitly `unmeasured` until a browser coverage producer exists; it is never inferred from a DOM observation.

The same browser run also produces live Delta Triangle evidence by replaying the exact head-selected program, seed, and sealed oracles against the merge-base runtime. The code axis is the intersection of that program's obligations, the flows that proved them, and the Weavatrix nodes that actually changed — not a repository-wide boolean. Playwright supplies the behavior axis. Preview-server origins are excluded from network identity, every observation is compared in order, and an incomplete replay or an unmeasurable code mapping is `unmeasured` rather than clean. A measured code + behavior change with no authorizing OpenSpec delta emits `WVQ-BEHAV-001` and blocks the ordinary composite verdict even if the sealed assertion still passes.

The spec axis is not a change-wide flag. WVQ reads the same OpenSpec change folder at the merge base, diffs it down to individual requirements and scenarios, and authorizes a program only when every obligation that program asserts falls inside that changed scope. A changed requirement body authorizes its own scenarios and nothing else; a changed scenario authorizes only itself; a change folder that did not exist at base authorizes everything it declares. Moving unchanged prose is not an intent change, and a program that mixes one changed obligation with one unchanged obligation is not authorized at all. The code axis is scoped the same way: a theme-file Weavatrix node does not satisfy a checkout program, and a missing obligation→flow→node mapping is recorded as `unmeasured` rather than inherited as a global `true`. The per-program spec and code decisions are recorded in the `delta-triangle` artifact.

Every attempted measured Playwright step now owns an exact start/end observation span. The bounded network journal assigns each request a monotonic identity of method, path, content type, and a canonical body digest — GraphQL adds operation name, query hash, and variables hash — and never records bodies or header values. WVQ waits for an immediate application-level mutation retry within a bounded action settle window, then ratchets base against head. Two identical mutating requests inside one action produce blocking `WVQ-UI-NET-001`; the same request in two separate action spans is preserved as two user intents and is not called a duplicate. A truncated or disabled journal is missing evidence, never a clean result.

The same v2 layout snapshot carries bounded accessibility facts without exporting markup or form values. Playwright measures computed role/name, label association, focusability, disabled and selected states, dialog modality/focus, and the exact semantic targets named by sealed predicates; Rust applies the rules and severity. Missing names or labels, inconsistent role/state, required-flow keyboard loss, and broken dialog semantics are base/head-ratcheted. A new error on a sealed target blocks the ordinary composite verdict, while unrelated accessibility debt is a warning instead of a surprise cleanup assignment for QA. This path uses zero runtime model and vision tokens.

For a package whose test script is exactly `vitest` or `vitest run`, WVQ selects the registered Vitest executor and adds the runner's built-in JUnit reporter automatically. It resolves the repository-local binary through offline, non-interactive npm execution, imports the report, and deletes only WVQ's private `.weavatrix-quality/junit.xml` before checking the revision. Repository-owned report paths are never removed. More complex package scripts stay on the generic npm boundary and can still contribute any fresh supported JUnit/LCOV artifacts they produce.

Repositories using Storybook's official Vitest addon get a separate bounded browser-project executor. An impacted run unions the base and head Weavatrix surfaces, selects an existing `.stories.*` file only when that story is in the union, and invokes `vitest run --project=storybook` rather than the ordinary Vitest project. When `@vitest/coverage-v8` is declared, the executor also emits LCOV; otherwise WVQ records the real browser case without pretending coverage exists. JUnit failures override a zero process exit code, so a reporter-only failure cannot become a false pass.

Mutation hints now have a real source-execution path. For changed non-test TS/JS and Go lines, WVQ plans only the bounded built-in catalogue, creates a detached worktree at the exact head commit, overlays the requested working-tree content when needed, applies one reversible edit, and runs only exact obligation-bound Go or Vitest/Storybook-Vitest cases. The user's checkout is never edited. A compile failure, missing report, ambiguous case identity, unsupported runner, or spent execution ceiling is `invalid`/`unmeasured`, never a killed mutant. Results are attributed per obligation: a test for one obligation cannot strengthen another. A surviving, invalid, or required-but-missing measurement turns an otherwise `PROVEN` proof into `PARTIAL`; forged counters, policy mismatches, and unknown artifact states fail closed. The producer caps one mutant at 120 seconds, the whole mutation phase at 600 seconds, source edits at 32, obligation-case decisions at 128, and output at 2 MiB per process. This path uses zero model tokens.

The built-in source catalogue is `boundary_flip`, `equality_flip`, `bool_flip`, `logical_flip`, `off_by_one`, `remove_branch`, `remove_sort`, `wrong_permission`, `omit_callback`, `omit_error`, and `collection_boundary` for TS/JS, plus `err_nil_flip`, `boundary_flip`, `return_zero`, `skip_branch`, `ignore_context`, and `invert_bool` for Go. Project-semantic hints such as `omit_group` are not guessed onto source syntax; they remain visible in the mutation artifact as unmapped limitations.

Measured coverage can teach later selections without replacing Weavatrix. WVQ attributes graph nodes to a test only when a successful executor invocation ran exactly one test path; aggregate coverage from multi-test batches is never guessed onto each member. A historical test enters the base/head candidate union only after the same test-node relation was observed in two distinct runs. The bounded `selection-decision` artifact shows every chosen path, its evidence chain, the observation floor, and uncovered obligations.

`wvq-bench` is also a defensive learning run, not just a timer. After the impacted and full scopes finish at the same change/revision, WVQ compares normalized failing test identities. The audit is `corroborated`, `contradicted`, `unmeasured`, or `not_reduced`; a failure found only by the full run is persisted as a `selection-audit` artifact and its safely resolved test path is fed into future selection for that impacted graph surface. Replaying the same run pair is idempotent, and bounded samples never hide the total miss count.

## CLI

```text
wvq spec validate [--change ID]
wvq spec seal [--change ID]
wvq analyze [--change ID] [--purpose spec|implementation|review] [--token-budget N]
wvq debt [--change ID]
wvq select [--change ID]
wvq recover [--change ID] [--base REF] [--head REF|WORKTREE]
wvq run [--change ID] [--scope impacted|all] [--evidence-policy standard|minimal|none]
wvq status
wvq verify [--change ID]
wvq explain <id>
wvq plan [--change ID]
wvq model [--change ID] --kind planning|runtime|browser_escape|vision --prompt TEXT
```

Unknown and duplicate flags fail instead of being silently ignored. A blocking `CONTRADICTED` verify verdict exits with code 2; unresolved evidence exits with code 1.

`wvq recover` is the bounded missing-intent path. When Weavatrix proves that an
exported function or method and a test changed without an OpenSpec delta, WVQ
prepares one revision-bound evidence packet and one candidate. Private helpers,
test declarations, and changes that already carry an OpenSpec delta are filtered
before review. The normal path spends zero model tokens; implementation and its
own changed test can only produce `QA_REVIEW` and can never auto-seal intent.

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

Mutation is enabled per scenario in its OpenSpec `quality.yaml`:

```yaml
requirements:
  - capability: permissions
    requirement: viewer-delete
    scenarios:
      - scenario: viewer-denied
        obligations:
          - id: viewer-cannot-delete
            kind: invariant
        evidence:
          required: []
          on_failure: []
        mutation:
          operators: [wrong_permission, boundary_flip]
```

An empty `operators` list requests every safe built-in for a compatible changed source file. Explicit names select only those built-ins; incompatible ecosystems are `not_applicable` rather than silently widened.

## JavaScript and npm

The `wvq` npm package is a typed JavaScript boundary around the same Rust implementation. It does not fork policy, graph, evidence, proof, or budget behavior into TypeScript. Release artifacts carry `wvq`, `wvq-mcp`, and `wvq-bench` for Windows, macOS, and Linux on x64 and arm64.

```sh
npm install --save-dev wvq
npx wvq --repo . plan --change current
npx wvq --repo . record --change current --route /dashboard
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
const recorded = await authoring.record({ route: '/dashboard', fixtureValues: { account: 'demo' } })
const healed = await authoring.heal(promoted.program_id, promoted.program_revision, [
  { edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' } },
])
```

All launchers use direct argv without a shell. The package includes MCP Registry metadata, and tag releases build and smoke-test all six platform variants before npm and MCP publication. Browser preview resolves Playwright from `browser.module_root`; install Playwright and the required browser engines in the repository that WVQ verifies.

## Repository policy

`.weavatrix-quality/config.yaml` binds concrete tests to compiled obligation IDs. A green runner without this mapping remains `UNPROVEN`.

```yaml
quality_policy_v: 1

test_bindings:
  - path: tests/permissions.spec.ts
    runner: playwright
    case: viewer cannot delete a widget
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
  network:
    mode: live # live, record, replay, or hybrid
    # profile: .weavatrix-quality/network/checkout.json # replay/hybrid only
    redact_json_keys: [customer_reference]
    max_entries: 256
    max_body_bytes: 65536
    max_total_bytes: 4194304
  programs: # optional while an agent is drafting its first TestProgram
    - .weavatrix-quality/programs/permissions.json

ui_integrity:
  enabled: true
  max_nodes: 5000
  geometry_tolerance_px: 1
  occlusion_failure_ratio: 0.5

  responsive:
    enabled: true
    min_width: 320
    max_width: 1440
    height: 720
    max_probes: 32

  allowed_overlaps:
    - top:
        role: tooltip
      bottom:
        role: button
      reason: tooltips intentionally cover their trigger

    - top:
        component_hint: Badge
      bottom:
        component_hint: Avatar
      reason: design-system unread badge

  accepted_text_truncation:
    - target:
        component_hint: TableCell
      requires_accessible_full_value: true

  exceptions:
    - fingerprint: ui:WVQ-UI-DUP-001:0f3c…
      reason: legacy widget scheduled for removal
      reviewer: qa@example.invalid
      expires: 2026-12-31
```

`path` is the file-selection identity. It is not proof by itself. An obligation is
proved only when WVQ finds the configured `case` in normalized evidence from the
required `runner`; optional `suite` disambiguates reporters that reuse case names.
A path-only binding satisfies neither obligation coverage nor proof, so impacted
execution widens safely and verification remains `UNPROVEN`.
Generic `npm test` scripts are always run unfiltered because WVQ cannot assume
that an arbitrary script honors positional file arguments.

`browser.network` virtualizes only same-origin `fetch`/XHR traffic; document,
asset, and cross-origin loading remain Playwright/browser behavior. `record`
captures a versioned JSON response profile as a CAS evidence handle, and the
passive `wvq record` path enables that capture automatically. Profiles never
contain request headers, cookies, or non-JSON bodies. JSON keys with built-in
sensitive/PII names, configured `redact_json_keys`, email-like values, bearer
tokens, and JWT-like strings are replaced before leaving the page boundary;
response count and byte ceilings are mandatory. `replay` fulfils recorded
method/path/query identities in order and aborts an unknown API request, which
fails the browser run even if an unrelated UI assertion still passes. `hybrid`
falls through to live traffic for unknown requests. Base/head comparison uses
the exact head-selected profile on both revisions, so a preview origin or
changing upstream cannot create a false behavioral delta.

`ui_integrity` is off unless the section is present, in which case the axis
reports `not_applicable` rather than a silent pass. When it is on, the bundled
Playwright bridge collects one bounded layout snapshot per executed step —
geometry, semantic identity, accessibility facts, and hit-test results, never
`innerHTML`, form values, cookies, or unbounded text — and Rust decides what
any of it means.
Detection costs zero model tokens and zero vision calls.

Responsive search is enabled with UI integrity by default. The browser exposes
the parsed width conditions from media rules, stylesheet `media` attributes,
and container queries; WVQ probes each boundary and its neighbours, then
bisects only measured base/head transitions down to one CSS pixel. It does not
run a fixed viewport matrix. `max_probes` is a per-revision browser-run budget;
exhausting it, or being unable to inspect an applied stylesheet, makes the axis
`unmeasured` rather than clean. A new error found only at a narrow width is
included in the ordinary `quality_verify` verdict with its exact measured
failure interval.

The section fails closed. An unknown field, an allowance that names no node, a
path-shaped matcher value, a ratio outside `0.0 … 1.0`, a malformed `expires`
date, or an exception without a reason is refused rather than ignored, and
there is no `accept_all`. Expired allowances stop applying and are reported so
they do not keep suppressing a finding unnoticed.

`quality_run` collects head evidence and replays the same selected programs at
the merge base automatically. That ordinary comparison turns UI and
accessibility findings into `new`, `existing`, `fixed`, or `returned`, persists
the delta for `quality_verify`, and returns its evidence handle. If either side
cannot be measured, the axis reports `unmeasured`; head evidence alone can
never claim that protection was preserved. `ui_integrity_view` remains a
detailed projection of the same evidence, not a prerequisite for the gate.

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

It exposes six high-level operations:

```text
quality_test_draft     complete sealed obligations + bounded changed-code/Weavatrix context
quality_test_validate  strict TestProgram validation against the existing OracleSeal; no writes
quality_test_preview   real Playwright run with observation/screenshot/trace handles; no test save
quality_test_promote   persist only the exact passing same-revision preview under the existing seal
quality_test_record    passive semantic session; discard duplicates and preview only useful replay candidates
quality_test_heal      replay locator/wait-only repair; append a version only when the old oracle passes
```

`quality_test_draft` normally uses zero model tokens and lets the calling agent produce the candidate. `use_model: true` explicitly requests one configured loopback planning call through the same persistent AI Cost Firewall. Candidate JSON cannot contain or replace oracle predicates, cannot use XPath, shell, JavaScript evaluation, filesystem writes, or unregistered cross-origin API operations. Screenshot capture defaults on for preview; trace capture is opt-in. `quality_test_record` opens a visible Playwright browser by default, captures natural click/select/fill/keyboard use as semantic targets, and finishes on inactivity or Ctrl+Shift+E. Raw form values never leave the page unless they exactly match an explicitly named replay fixture. Rust evaluates existing sealed predicates at the exact final state, measures new states, non-loop edges, API operations, and obligation links, discards a session that contributes none of them, and replays a useful `source: recorded` candidate through the same preview admission. It spends zero model tokens and still requires explicit promotion. Promotion revalidates the current program, change, repository revision, and `OracleSeal`, is idempotent for the same preview, and stores canonical JSON in CAS as program revision 1. Later `select` and `run` load the latest matching sealed revision automatically; a stale seal is never executed. Healing accepts only semantic retargeting and typed deterministic waits, uses optimistic revision concurrency, replays the unchanged assertions through Playwright, and writes a new version only after a pass. A failed replay returns evidence handles and leaves the active program unchanged.

`qualityd` serves the exception-first Studio API over the same command bus. The corresponding authoring endpoints are `POST /api/v1/authoring/draft`, `/validate`, `/preview`, `/record`, `/promote`, and `/heal`. Its dashboard hides ordinary passing proof noise while drill-down keeps the full evidence trail.

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
