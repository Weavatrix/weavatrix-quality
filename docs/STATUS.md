# STATUS — Weavatrix Quality

Last updated: 2026-08-21
Session: exact execution evidence and merge-base provenance

## Now

The 35 development-plan tasks are implemented, but task completion is not used as a synonym for production maturity. A domain contract, a library algorithm, a wired producer, and measured real execution are separate states.

### Maturity matrix

`✅` means the column is implemented and exercised. `🟡` means the path is partial, opt-in, or depends on repository-supplied evidence. `❌` means that layer is not implemented.

| Capability | Contract | Library | Wired | Real execution |
| --- | --- | --- | --- | --- |
| OpenSpec parser | ✅ | ✅ | ✅ | ✅ |
| `quality.yaml` | ✅ | ✅ | ✅ | ✅ |
| OracleSeal integrity | ✅ | ✅ | ✅ | ✅ |
| Executable oracle | ✅ | ✅ | ✅ | ✅ Playwright |
| Committed/worktree revision range | ✅ | ✅ | ✅ | ✅ base SHA + head SHA + merge-base |
| Weavatrix embed | ✅ | ✅ | ✅ | ✅ |
| Quality Debt Ratchet | ✅ | ✅ | ✅ | ✅ |
| Test selection | ✅ | ✅ | ✅ | ✅ Cargo/Vitest shadow runs |
| Exact case-level proof binding | ✅ | ✅ | ✅ | ✅ Cargo/Playwright; 🟡 repository JUnit/Go |
| Runner-specific file filtering | ✅ | ✅ | ✅ | ✅ Vitest/Jest/Bun/Playwright; generic npm widens |
| Executor registry | ✅ | ✅ | ✅ | ✅ Cargo/npm/Vitest/Jest/Bun/Go/Playwright |
| Runner normalization | ✅ | ✅ | ✅ | ✅ Cargo/JUnit/Go/browser |
| SQLite/CAS | ✅ | ✅ | ✅ | ✅ |
| Proof assembly and provenance | ✅ | ✅ | ✅ | ✅ exact case/assertion + CAS links |
| MCP default surface | ✅ | ✅ | ✅ | ✅ |
| AI Cost Firewall | ✅ | ✅ | ✅ | ✅ loopback local model |
| BehaviorGraph | ✅ | ✅ | ✅ | ✅ browser observations |
| Delta Triangle | ✅ | ✅ | ❌ default | ❌ live base/head verdict |
| Spec Recovery | ✅ | ✅ | ✅ opt-in | ✅ Git + Weavatrix, QA-gated |
| Test lineage | ✅ | ✅ | ✅ protection path | ✅ measured coverage path |
| ProtectionSnapshot/Delta | ✅ | ✅ | ✅ opt-in | ✅ base/head coverage replay |
| Protection MCP/Studio | ✅ | ✅ | 🟡 opt-in profile/view | ✅ when measured coverage exists |
| Real Playwright TestProgram | ✅ | ✅ | ✅ | ✅ actions + sealed assertions + observations |
| Manual record → promoted replay | ✅ | 🟡 | 🟡 preview/promotion | 🟡 replay yes, manual recorder no |
| Mutation | ✅ | ✅ model | ❌ producer | ❌ source mutation |
| Metamorphic | ✅ | ✅ primitive | ❌ project adapter | ❌ project execution |
| Cheap explorer | ✅ | ✅ planner | ❌ browser feedback | ❌ closed loop |
| Studio API | ✅ | ✅ | ✅ | ✅ local HTTP |
| Studio frontend | ✅ idea | ❌ | ❌ | ❌ |
| Composite ChangeQualityVerdict | ✅ idea | ❌ | ❌ | ❌ |

Current implementation: exact execution evidence and merge-base proof provenance are committed on `main` as `43d6e02`; cross-platform Cargo evidence hardening is `adfe53b`. The previous published live vertical through BehaviorGraph (`7ed6db4`) passed GitHub Actions run [32412625159](https://github.com/sergii-ziborov/weavatrix-quality/actions/runs/32412625159) across clean-checkout workspace, Playwright, typed JavaScript, installable-package smoke, and Clippy checks.

Local validation: 366 Rust tests, 7 Playwright-runner tests, and 5 npm package tests passed with zero failures. Workspace Clippy passed for all targets with warnings denied.

`quality_verify` no longer turns a green file path into obligation proof. A configured binding must name a runner and exact case that appears in normalized evidence. Cargo/libtest output is now normalized to target + case identities; browser proof records the exact assertion step and its corresponding observation. A missing case remains `UNPROVEN`, and an otherwise passing bound case cannot hide a failed runner invocation.

Cargo execution fixes `--color never`, while the normalizer also strips ANSI control sequences defensively. This keeps exact case evidence stable when CI forces colored Cargo output.

Every Git-backed command now resolves the requested base commit, checked-out head commit, and their merge-base. All diffs, Weavatrix base/head operations, recovery ranges, and base protection replay use that common ancestor. `RunReply`, execution evidence, and immutable proof provenance retain all three identities. Proof-to-artifact links are inserted atomically instead of being discarded after in-memory assembly.

The authoring vertical, actual selected-vs-full benchmark runner, typed JS/npm distribution, safe healing, live analytics/selection feedback, and Playwright-backed BehaviorGraph producer are all published on `main`. The benchmark executes both scopes through `LiveService`; the previous labelled fixture costs are no longer presented as measured wall-clock time.

Live test analytics are now wired rather than model-only. Every normalized JUnit/Go case and typed browser program is persisted with exact run/revision identity, outcome, and duration. Failure occurrences use stable fingerprints and the deterministic flake triage; mixed pass plus fail/error history is the only condition that marks an identity flaky. Each run emits a bounded CAS-backed `test-analytics` artifact with current outcomes, failure clusters, flaky histories, and the twenty slowest reported durations with historical means. `RunReply` exposes recorded/failed/flaky/unknown counts. This path uses zero runtime LLM tokens.

Playwright observations now feed the persistent BehaviorGraph on the normal live run path. Canonical route + accessibility digest + viewport states share one hash representation with CAS, and adjacent observations persist edges labelled by the exact typed `TestAction`. Run-local and newly admitted state/edge totals are separate in `RunReply`; a bounded same-revision `behavior-contribution` artifact links state digests to program obligations and observed API metadata. Missing browser coverage remains explicitly `unmeasured` rather than being invented from DOM evidence. Repeating the same behavior produces zero new states and edges, and the producer uses zero runtime LLM tokens.

Selection now consumes conservative measured history as a real producer. Successful coverage is attributed to a test-node pair only when an executor ran exactly one selected test path; a multi-test batch never teaches ambiguous per-test coverage. The pair must be seen in two distinct run ids before it joins the Weavatrix base/head union, and duplicate ingestion from one run cannot increase confidence. Candidate queries and persisted evidence are bounded; a `selection-decision` CAS artifact records the algorithm, selected paths, explanations, history count, observation floor, and uncovered obligations.

The live shadow benchmark now closes the defensive-learning loop. It requires an impacted run and an effective full run for the same change/revision, compares normalized failing identities, and persists one idempotent audit as `corroborated`, `contradicted`, `unmeasured`, or `not_reduced`. A full-only failure remains visible even if its suite cannot be mapped safely; an exact repository test path is fed back into future selection immediately for the bounded impacted-node set. The `selection-audit` CAS artifact reports total misses, bounded identity/path samples, truncation, and zero runtime LLM tokens.

Direct Vitest package scripts now produce measured cases without repository configuration. Discovery promotes only exact `vitest` or `vitest run` scripts with a declared Vitest dependency to the frozen runner. The executor resolves the local package binary through `npm exec --offline --yes=false`, enables Vitest's built-in JUnit reporter, uses the existing private evidence directory for root or nested packages, imports the fresh report, and removes the generated file before revision validation. Repository-owned JUnit paths are untouched; stale private output is removed before every invocation.

The real TS-frontend shadow probe compiled 8 obligations and 289 bounded context items from a 17-file change packet. Drafting used 7,978/8,000 tokens, truncated only non-authoritative context, and made no model call. MCP validation binds a generated program to the existing `OracleSeal`; persistence is now a separate passing-preview admission step.

The first runtime probe exposed and fixed Windows short-path propagation into Vitest. A later competitor pass found that file paths were still represented as one test-title filter per process. Runner-aware batching now keeps paths as positional argv (or Jest `--runTestsByPath`) and combines bounded paths for the same runner. On the latest repeat of the same broad graph case, impacted execution selected 41 of 42 available test files in one process, normalized 203 passing cases, and completed in 48.26 s; full execution selected 42 of 42 in one process, normalized the same 203 passing cases, and completed in 47.79 s. The defensive audit is `corroborated` with zero missed failures. This is a measured one-file reduction with no speedup in this sample—the impacted run was 0.47 s slower. Both scopes used zero runtime LLM tokens. `scope_reason` and explicit selected/available/invocation counts make that visible instead of conflating “filter applied” with “time saved”.

## Playwright authoring path

| Operation | Live behavior |
| --- | --- |
| draft | Resolves an explicit base/head range, requires changed code, queries same-revision `graph_diff` and change impact, returns bounded intent/graph context, and never truncates the sealed obligation authority |
| optional model | `use_model: true` performs one planning call through the existing loopback-only persistent AI Cost Firewall; normal draft and verification use zero model tokens |
| validate | Strictly decodes canonical `TestProgram` JSON, rejects candidate-owned oracle fields, unknown/duplicate obligations, XPath, unknown actions, and obligations without an executable sealed expected predicate |
| preview | Executes actual Playwright (`chromium`, `firefox`, or `webkit`), checks repository revision before/after, imports observations/screenshots/trace into CAS, removes exact temporary evidence files, and records a passed/failed admission identity without registering the candidate |
| promote | Revalidates the exact previewed program, current repository revision, change, and existing `OracleSeal`; only a passing preview atomically becomes CAS-backed program revision 1, and repeated promotion is idempotent |
| reuse | `select` and `run` automatically load the latest promoted revision whose seal still matches; stale-seal programs are not executed, and a repository-configured program cannot silently shadow the same id |
| heal | Accepts only semantic retargeting or typed deterministic waits, requires the caller's latest program revision and the same `OracleSeal`, runs real Playwright with the original assertions, and atomically appends a CAS-backed revision only on pass; failed repairs retain evidence but do not replace the active program |
| transports | MCP: `quality_test_{draft,validate,preview,promote,heal}`; HTTP: `POST /api/v1/authoring/{draft,validate,preview,promote,heal}` |

Affected-package validation: 107 tests passed with zero failures, including the real Rust → stdio bridge → Playwright preview with two screenshots and a trace. Clippy passed for `wvq-runtime`, `wvq-command-bus`, `wvq-mcp`, and `qualityd`, all targets, with warnings denied.

Current validation: 366 Rust tests, 7 Playwright-runner tests, and 5 JS package tests pass with zero failures. Public TypeScript declarations compile in strict `NodeNext`; workspace Clippy passes for all targets with warnings denied. The real shadow benchmark also passed both sequential scopes and produced a corroborated normalized-case audit.

## JS/npm distribution

- Package name: `wvq`; the JS API is a typed, no-shell boundary over Rust rather than a second policy implementation.
- `npx wvq`, `npx wvq mcp`, and `npx wvq bench` select only the three fixed native programs. Direct `wvq-mcp` and `wvq-bench` bins are also present after installation.
- `WvqClient` covers the CLI command bus; `WvqMcpClient` provides generic bounded calls plus typed authoring `draft`, `validate`, `preview`, `promote`, and `heal` helpers.
- The package resolves bundled Windows/macOS/Linux x64/arm64 programs, verified platform packages, explicit binary overrides, or the local workspace binaries. It never falls back through a shell or recursively launches itself from `PATH`.
- The tag workflow builds and smoke-tests all three programs on six platform runners, assembles the universal package, installs it into a clean prefix, exercises a real MCP call, publishes npm with provenance, validates/publishes official MCP metadata, verifies both registries, and then creates an immutable GitHub release.
- `server.json` and npm `mcpName` are version-locked. The official MCP Registry publisher accepted the current metadata locally.

Distribution validation: 5 JS behavior/metadata tests passed, the public TypeScript declarations compiled under strict `NodeNext`, the current Windows package contained all three native programs plus `server.json`, all launchers returned success, npm dry-run passed, and `actionlint` accepted both workflows.

## Live production path

| Producer | Live behavior |
| --- | --- |
| execution | Discovers Cargo/npm/Vitest/Jest/Bun/Go/Playwright manifests and invokes only frozen bounded executor definitions |
| selection | Combines Weavatrix head impact, base-only removed test evidence, and explicit obligation bindings; incomplete/unsafe filters widen to the full suite |
| selection history | Learns only repeated, single-test measured coverage and unions it with base/head evidence; ambiguous batches never train the selector |
| defensive audit | Compares impacted vs full failing identities, persists misses, and feeds exact missed test paths back into the selector |
| graph impact | Persists `graph_diff`, change impact, static selection, and `Impact(base) ∪ Impact(head) ∪ removed` at one exact revision |
| coverage | Normalizes fresh LCOV/Go evidence, maps it to changed graph nodes, and persists a revision-bound `ProtectionSnapshot` |
| evidence | Stores run/items, raw streams by policy, normalized results, semantic maps, summaries, and large blobs through SQLite + CAS |
| test analytics | Persists exact test identity/outcome/duration history, fingerprints failures, identifies mixed-history flakes, and emits a bounded CAS report without an LLM call |
| behavior | Converts real Playwright observations into canonical persistent states and typed adjacent edges, reports novelty separately, and links obligations/API metadata without inventing coverage |
| proof | Uses only the latest same-change, same-revision run; proof requires an exact normalized runner case or browser assertion, and persists the linked revision/evidence artifacts |
| debt | Uses immutable base/head Weavatrix evidence and persistent fixed-history to classify `new/existing/fixed/returned/excepted` |
| AI | Explicit opt-in loopback completion path, preflight reservation, server usage evidence, global + change-local ceilings, persistent per-change spend |

`plan` reads existing same-revision proofs. `explain` resolves obligations, proofs, runs, selections, and debt findings with provenance. `status`, evidence handles, proofs, debt history, and AI usage survive a new process.

## Safety invariants exercised

- no arbitrary shell over MCP;
- large artifacts remain handles;
- unknown schema versions, command values, stale/malformed evidence, revision drift, and incomplete graph diffs fail closed;
- missing coverage is unmeasured, never uncovered;
- a successful unbound suite remains `UNPROVEN`;
- normal verification makes no model call and spends zero runtime tokens;
- model calls accept loopback HTTP only and are refused before network I/O when budget cannot cover the reservation;
- detector blocking requires `High` weight and per-signal `Confirmed` graph corroboration.

## Measured detector calibration

On sixty accepted, defect-free changes, text matching fired on 33–92% depending on category and the initial policy would have blocked 42% of clean changes. Graph-backed default-flip and retired-persisted-key categories fired on 5–8%. The graph promotes only the signal whose concrete symbol it names; `TestMovedWithImplementation` is never promoted.

## Repository maintenance debt

- `cargo fmt --all -- --check` has roughly forty pre-existing formatting differences. Repo-wide formatting remains deferred so unrelated churn is not mixed into the release; touched code passes workspace Clippy with warnings denied.
- `[profile.dev] debug = "line-tables-only"` remains in the workspace manifest to keep local build artifacts bounded.

## Load next

Build the composite change verdict and make measured protection loss part of default `quality_verify`; then prove it with the committed base/head monorepo fixture. After that, finish the live mutation producer and broader platform/hosted UX. Do not duplicate Rust policy/proof semantics in TypeScript.
