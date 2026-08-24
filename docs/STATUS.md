# STATUS — Weavatrix Quality

Last updated: 2026-08-24
Session: changed-public-symbol recovery through the committed B5 product path

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
| Weavatrix embed | ✅ | ✅ | ✅ | ✅ 2.7.1 |
| Quality Debt Ratchet | ✅ | ✅ | ✅ | ✅ |
| Test selection | ✅ | ✅ | ✅ | ✅ Cargo/Vitest shadow runs |
| Exact case-level proof binding | ✅ | ✅ | ✅ | ✅ Cargo/Playwright; ✅ unambiguous single-case JUnit/Go; 🟡 aggregate coverage |
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
| Test lineage | ✅ | ✅ | ✅ protection path | ✅ exact single-case measured coverage; aggregate runs stay executor-level |
| ProtectionSnapshot/Delta | ✅ | ✅ | ✅ default verdict axis; base replay stays explicit | ✅ base/head coverage replay |
| Protection MCP/Studio | ✅ | ✅ | ✅ verdict axis + Studio summary; 🟡 opt-in profile/view | ✅ when measured coverage exists |
| Real Playwright TestProgram | ✅ | ✅ | ✅ | ✅ actions + sealed assertions + observations |
| Manual record → promoted replay | ✅ | 🟡 | 🟡 preview/promotion | 🟡 replay yes, manual recorder no |
| Mutation | ✅ | ✅ model | ❌ producer | ❌ source mutation |
| Metamorphic | ✅ | ✅ primitive | ❌ project adapter | ❌ project execution |
| Cheap explorer | ✅ | ✅ planner | ❌ browser feedback | ❌ closed loop |
| Studio API | ✅ | ✅ | ✅ | ✅ local HTTP |
| Studio frontend | ✅ idea | ❌ | ❌ | ❌ |
| Composite `ChangeQualityVerdict` | ✅ | ✅ | ✅ default `quality_verify` | ✅ live PASS / BLOCKED / NOT_ENOUGH_EVIDENCE |
| Protection as a default verdict axis | ✅ | ✅ | ✅ | ✅ from stored base/head snapshots |
| Committed product fixture | ✅ A/B1–B5 | ✅ A/B1–B5 | ✅ CLI/MCP/Studio for B2/B4/B5 | ✅ A/B1–B5 |
| Sealed UI predicates | ✅ | ✅ | ✅ | ✅ Chromium, each asserted both ways |
| `LayoutSnapshot` v1 | ✅ | ✅ | ✅ | ✅ bounded, redacted, settle-checked |
| UI Integrity detectors | ✅ | ✅ | ✅ | ✅ Chromium base/head fixture |
| UI Integrity ratchet | ✅ | ✅ | ✅ | ✅ new/existing/fixed/returned/excepted |
| UI Integrity in MCP/Studio | ✅ | ✅ | ✅ | ✅ `quality_verify`, `quality_explain`, summary |
| React render profiler | ✅ idea | ❌ | ❌ | ❌ |
| Duplicate mutation requests | ✅ idea | ❌ | ❌ | ❌ deferred: no action-span protocol |
| Responsive interval search | ✅ idea | ❌ | ❌ | ❌ |

Current implementation: the composite change verdict and deterministic UI-integrity axis are on `main`. The current tree embeds `weavatrix-rust` 2.7.2 and adds a clean committed A/B1/B2/B3/B4/B5 product fixture across React/Vitest/Playwright, Node, Go, and nested OpenSpec. Exact execution evidence and merge-base proof provenance remain as `43d6e02`; cross-platform Cargo evidence hardening is `adfe53b`. The previous published committed-protection vertical (`5e2e167`) passed GitHub Actions run [32708728281](https://github.com/Weavatrix/weavatrix-quality/actions/runs/32708728281) across clean-checkout workspace, Playwright, typed JavaScript, installable-package smoke, and Clippy checks.

Pre-B5 full validation: 493 Rust tests passed with zero failures, and workspace Clippy passed for all targets with warnings denied. The B5 path separately passed the complete touched-crate suite (`wvq-command-bus`, `wvq-spec-recovery`, and `wvq-cli`, including all committed product scenarios) plus warnings-denied Clippy. The prior 20 Playwright-runner tests, 5 npm package tests, and strict public `NodeNext` declarations remain unchanged by this Rust-owned path.

## Composite change verdict

`quality_verify` no longer aggregates `ProofVerdict` alone. Proof, protection, debt, stability, AI budget, and UI integrity are separate axes joined by a fixed priority order; each keeps its own facts and provenance and there is no opaque score. `blocking` and the process exit code follow the composite state, while the `verdict` token stays backward compatible.

An axis reports `not_applicable` when the change has no surface it can measure and `unmeasured` when it has that surface and the evidence is absent. The distinction is the point: missing evidence never becomes a pass, and an axis that was never in scope is not reported as a gap.

Priority order, most important first: an active sealed-oracle contradiction; lost critical protection; new blocking architecture, API, or security debt, or a new blocking UI regression; a mandatory obligation left unproven; the same two classes returned after being fixed; a mandatory test with an unresolved new flake or an ambiguous specification; a required axis that was not measured; an AI budget exhausted with a mandatory decision still open; warning-only drift. The first rule that fires decides the state; every other fired rule stays listed.

Three invariants are asserted rather than assumed: a `PROVEN` behavioural proof cannot suppress a lost protection delta, a global coverage gain cannot suppress a local protection loss, and missing evidence is never a pass. Live fixtures cover all three outcomes — a healthy Cargo change composes to `PASS`, a change that deletes the only test reaching `subtract` is `BLOCKED` while its suite is green, and head coverage with no base snapshot is `NOT_ENOUGH_EVIDENCE` at exit code 1.

Protection is a default axis. `quality_verify` composes it from the head snapshot every run already persists plus the base snapshot `protection_view` now stores against that run, so no caller attaches a `ProtectionView` and `verify` still executes nothing.

## Committed monorepo protection fixture

The product fixture creates a real temporary Git monorepo with two clean commits and a clean worktree. It contains a React/Vitest frontend with V8 LCOV, a Node backend, a Go service with `go test -json` plus a coverprofile, a Playwright `TestProgram` executed in Chromium, and nested OpenSpec plus `quality.yaml`. All evidence is produced through registered executors; no fixture inserts a synthetic `ProtectionSnapshot`.

- **A → B1, healthy refactor:** implementation and its exact protector move files and the function moves to a different source line. Symbol identity is relocated without using the old path or source position, measured protection is preserved, and the composite verdict passes.
- **A → B2, phantom protector:** the exact Go case still passes but stops executing `CanDelete`; an unrelated covered symbol cannot hide the loss. The optional protection view emits `WVQ-PROTECT-002`, and the same stored loss blocks ordinary `quality_verify`, actual MCP JSON-RPC, and the Studio summary while behavioural proof remains `PROVEN`.
- **A → B3, deleted protector:** the sole exact protector disappears while the remaining suite is green. Both the detailed view and default verdict retain `WVQ-PROTECT-003` and block.
- **A → B4, intended expectation replacement:** OpenSpec, its compiled obligations, the Go protector, and the Playwright assertion change from viewer denial to viewer allowance. WVQ stores one immutable proposal containing base/head/merge-base, the exact Weavatrix content revision, both full seal digests, and the explicit obligation mapping. A stale digest or developer acceptance cannot authorize it. One exact QA or product-owner acceptance makes the old helper paths obsolete, the new protector `REPLACED`, and CLI, MCP, and Studio return `PROVEN` with a non-blocking composite state and zero runtime model tokens.

Coverage is assigned to an exact case only when normalized evidence contains one passing case and its runner, suite, and case match a repository binding. Multi-case coverage remains executor-level evidence; WVQ does not guess which case reached a symbol. Current Weavatrix nested spans are read at symbol granularity, and the file node is used only as a fallback when that source has no symbol spans.

**A → B5, missing declared intent:** the Go `CanDelete` implementation and its exact protector change while nested OpenSpec does not. Weavatrix 2.7.2 identifies the concrete exported function from a full declaration fingerprint; WVQ filters private helpers and test declarations, prepares one bounded candidate, and exposes the same `QA_REVIEW` state through `wvq recover`, the opt-in MCP recovery profile, and Studio. The changed implementation plus its own test is marked as a weak oracle, requires intent-owner escalation, spends zero model tokens, and cannot auto-seal. A real OpenSpec delta suppresses this recovery candidate instead of asking for redundant review.

## UI integrity

`wvq-ui` is a pure-Rust crate: no browser, no DOM, no model. Collection stays in `js/playwright-runner`, orchestration in `wvq-command-bus`, evidence in `wvq-store`, sealed expectations in `wvq-spec`, so detector logic exists exactly once.

`LayoutSnapshot` v1 is deliberately not a DOM. It carries geometry, semantic identity, and hit-test results — never `innerHTML`, form values, cookies, storage contents, response bodies, or unbounded text. Labels are collapsed and cut to 120 characters in the page before they leave it, and `textContent` only names an element that is a control or a leaf, so a list row never copies the text of everything inside it.

Collection is deterministic or it says so. Fonts are awaited with a bounded timeout, animations and transitions are frozen and driven to their end state, and the page is read twice: two reads that disagree beyond tolerance mark the snapshot unsettled instead of trusting one of them. Geometry and hit testing happen in a single `evaluate`, so both describe one DOM state and node identities are derived once. Node, hit-test-sample, candidate-pair, artifact-byte, and label bounds are all explicit, and hitting any of them sets `truncated`, which propagates into the verdict as a limitation.

Seven detectors ship: `WVQ-UI-DUP-001` duplicate DOM identity, `WVQ-UI-DUP-002` duplicate test identity, `WVQ-UI-DUP-003` ambiguous interactive identity, `WVQ-UI-LAYOUT-001` interactive occlusion, `WVQ-UI-LAYOUT-002` viewport overflow, `WVQ-UI-LAYOUT-003` text clipping, and `WVQ-UI-LAYOUT-004` confirmed control overlap.

Most of the work is refusing false positives, and each exclusion is a specific rule rather than a confidence threshold: repeated row actions are separated by entity scope; a control's own children never occlude it; `pointer-events: none` layers never intercept; ancestor/descendant containment is structure, not collision; content inside a scroll container is reachable by scrolling; an accepted ellipsis still needs the accessible full value to be present; and geometric overlap with no hit-test confirmation is not reported at all. When no row or dialog scope can be resolved, an ambiguous pair drops to a warning instead of blocking.

The ratchet compares the same program, at the same step, on the same route, at the same viewport. That key is built from the program rather than the accessibility digest on purpose: the digest changes whenever the markup does, which is exactly what a regression is, so using it would make every regression incomparable. Old debt is counted and never blocks adoption; a fixed finding is credited and remembered, so reintroducing it later is `returned` rather than `new`. A state only one revision measured is reported as unmeasured instead of being claimed as new.

Overlap candidates come from a sweep line, not a pairwise scan.

## Measured UI-integrity cost

| Measurement | Result |
| --- | --- |
| Browser collection, 56-node page | 82 ms for two settle-checked reads plus 148 hit-test probes |
| Snapshot size, same page | 31 828 bytes, about 568 bytes per collected node |
| Extrapolated to the 5 000-node default ceiling | about 2.8 MB, inside the 4 MiB artifact ceiling |
| Sweep over 5 000 tiled nodes | 0 intersecting pairs in 2.0 ms; a full scan would compare 12 497 500 |
| Full detection pass, 5 000 nodes | 10.3 ms release, on a synthetic worst case producing 4 560 findings |
| Dense 800-box pile | Bounded at 200 000 pairs and reported as truncated, not silently dropped |
| `ui_integrity_view` on the Chromium fixture | 4.0 s for the head run plus the base worktree replay |
| Runtime model tokens, whole path | 0 |
| Vision calls | 0 |

Detection is roughly two orders of magnitude cheaper than collection, so the cost of the axis is browser time, not analysis time. The per-node byte figure is what the `max_nodes` default is calibrated against.

## Proven UI-integrity scenario

The sealed behavioural test passes. The change introduces an overlay over the Export button. The base revision had no occlusion. Head has deterministic geometry and hit-test evidence, WVQ stores the artifacts, identifies the new regression, `quality_explain` names the exact target, occluder, route, viewport, and probe counts, and composite `quality_verify` returns `BLOCKED` with `verdict` still `PROVEN` and zero runtime model tokens.

That runs against real Chromium and two real revisions through the product path. Four further variants run against the same base: a duplicate `Save` in one dialog scope, horizontal overflow at 767 px, a clipped critical label reported with its measurements, and the two cases that must stay clean — repeated `Delete` buttons in separate row scopes, and a declared tooltip overlap.

Browser tests now take a cross-process lock. Cargo runs each integration-test file as its own process, so the previous in-process mutex left several binaries launching Chromium at once; that, plus a 30-second fixture deadline, is what made browser tests fail intermittently under `cargo test --workspace`. The fixtures now allow the full 120-second bridge budget.

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

Current validation: 493 Rust tests pass with zero failures, and workspace Clippy passes for all targets with warnings denied. The prior 20 Playwright-runner tests, 5 JS package tests, strict public `NodeNext` declarations, and corroborated real shadow benchmark remain unchanged by the B4 path.

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
| ui integrity | Collects one bounded, settle-checked layout snapshot per step, runs the detectors in Rust, and persists `ui-layout-snapshot`, `ui-hit-test-map`, and `ui-integrity-findings`; `ui_integrity_view` replays the base and stores the classified `ui-integrity-delta` |
| proof | Uses only the latest same-change, same-revision run; proof requires an exact normalized runner case or browser assertion, and persists the linked revision/evidence artifacts |
| expectation replacement | Compares base/head `OracleSeal` documents, persists one immutable revision-bound proposal, refuses stale or developer-only approval, and feeds an exact QA/product-owner acceptance into the default protection verdict |
| debt | Uses immutable base/head Weavatrix evidence and persistent fixed-history to classify `new/existing/fixed/returned/excepted` |
| AI | Explicit opt-in loopback completion path, preflight reservation, server usage evidence, global + change-local ceilings, persistent per-change spend |
| verdict | Composes proof, protection, debt, stability, AI, and UI integrity into one ranked change-level state from stored evidence, without executing anything |

`plan` reads existing same-revision proofs. `explain` resolves obligations, proofs, runs, selections, debt findings, and UI-integrity findings with provenance; a UI explanation names the target, the occluding or duplicate counterpart, the route and viewport, the exact probe and geometry numbers, and the artifact handles. `status`, evidence handles, proofs, debt history, and AI usage survive a new process.

## Safety invariants exercised

- no arbitrary shell over MCP;
- large artifacts remain handles;
- unknown schema versions, command values, stale/malformed evidence, revision drift, and incomplete graph diffs fail closed;
- missing coverage is unmeasured, never uncovered;
- a successful unbound suite remains `UNPROVEN`;
- normal verification makes no model call and spends zero runtime tokens;
- model calls accept loopback HTTP only and are refused before network I/O when budget cannot cover the reservation;
- detector blocking requires `High` weight and per-signal `Confirmed` graph corroboration;
- an axis with no surface is `not_applicable` and an axis with no evidence is `unmeasured`; neither is reported as clean;
- a truncated or unsettled layout snapshot is never a clean measurement;
- the UI policy refuses unknown fields, empty matchers, path-shaped values, out-of-range ratios, malformed dates, exceptions without a reason, and any `accept_all`;
- every sealed predicate must be executable in the browser, enforced by a parity test over all 24 variants;
- UI collection persists no raw markup, form values, cookies, storage contents, or unbounded text.

## Measured detector calibration

On sixty accepted, defect-free changes, text matching fired on 33–92% depending on category and the initial policy would have blocked 42% of clean changes. Graph-backed default-flip and retired-persisted-key categories fired on 5–8%. The graph promotes only the signal whose concrete symbol it names; `TestMovedWithImplementation` is never promoted.

## Repository maintenance debt

- `cargo fmt --all -- --check` has roughly forty pre-existing formatting differences. Repo-wide formatting remains deferred so unrelated churn is not mixed into the release; touched code passes workspace Clippy with warnings denied.
- `[profile.dev] debug = "line-tables-only"` remains in the workspace manifest to keep local build artifacts bounded.

## Load next

Close the remaining committed product-fixture paths before adding another feature family:

1. **Executed-protector inventory.** Store exact normalized test identities independently of covered flows, so a surviving case that reaches no impacted symbol is still classified as phantom instead of merely absent.
2. **Responsive failure interval search.** Bisect around CSS and container breakpoints instead of testing a fixed viewport list, so the exact width a control breaks at is reported rather than guessed.
3. **Storybook/Vitest impacted-story adapter.** Reuse the existing impacted-surface union to pick affected stories, giving UI integrity a cheap per-component measurement point next to the whole-page one.

After that: add the typed action-span protocol before duplicate-mutation detection, then finish the live mutation producer, metamorphic project adapter, closed-loop cheap explorer, responsive UI wave, and Studio frontend. Do not duplicate Rust policy or proof semantics in TypeScript, and do not add a default MCP tool for UI detail — `quality_verify`, `quality_explain`, and `quality_evidence` already carry it.
