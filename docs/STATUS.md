# STATUS — Weavatrix Quality

Last updated: 2026-08-25
Session: P1 — wire ApplicationSurfaceGraph as a read-only MCP/Studio view

## Now

The 35 development-plan tasks are implemented, but task completion is not used as a synonym for production maturity. A domain contract, a library algorithm, a wired producer, and measured real execution are separate states.

`ApplicationSurfaceGraph` is now a live run artifact (`application-surface-graph`) and a read-only MCP/Studio projection (`protected` / `partial` / `unmeasured`). It is not a gate: `quality_verify` still composes from the existing axes. Missing coverage stays unmeasured, never a clean empty graph; instrumented zeros fold into `unmeasured` on the product surface; unknown schema versions fail closed. `quality_explain` names a surface; Studio copies the same three lists onto the change summary.

### Maturity matrix

`✅` means the column is implemented and exercised. `🟡` means the path is partial, opt-in, or depends on repository-supplied evidence. `❌` means that layer is not implemented.

| Capability | Contract | Library | Wired | Real execution |
| --- | --- | --- | --- | --- |
| OpenSpec parser | ✅ | ✅ | ✅ | ✅ |
| `quality.yaml` | ✅ | ✅ | ✅ | ✅ |
| OracleSeal integrity | ✅ | ✅ | ✅ | ✅ |
| Executable oracle | ✅ | ✅ | ✅ | ✅ Playwright |
| Committed/worktree revision range | ✅ | ✅ | ✅ | ✅ base SHA + head SHA + merge-base |
| Weavatrix embed | ✅ | ✅ | ✅ | ✅ 2.7.4 |
| Quality Debt Ratchet | ✅ | ✅ | ✅ | ✅ |
| Test selection | ✅ | ✅ | ✅ | ✅ Cargo/Vitest shadow runs |
| Exact case-level proof binding | ✅ | ✅ | ✅ | ✅ Cargo/Playwright; ✅ unambiguous single-case JUnit/Go; 🟡 aggregate coverage |
| Runner-specific file filtering | ✅ | ✅ | ✅ | ✅ Vitest/Jest/Bun/Playwright; generic npm widens |
| Executor registry | ✅ | ✅ | ✅ | ✅ Cargo/npm/Vitest/Storybook/Jest/Bun/Go/Playwright |
| Process-tree kill | ✅ | ✅ | ✅ deadline/cancel/output | ✅ Unix process group + Windows job object |
| Impacted Storybook/Vitest | ✅ | ✅ | ✅ | ✅ official addon + Playwright Chromium + JUnit/LCOV |
| Runner normalization | ✅ | ✅ | ✅ | ✅ Cargo/JUnit/Go/browser |
| SQLite/CAS | ✅ | ✅ | ✅ | ✅ |
| Proof assembly and provenance | ✅ | ✅ | ✅ | ✅ exact case/assertion + CAS links |
| MCP default surface | ✅ | ✅ | ✅ | ✅ |
| AI Cost Firewall | ✅ | ✅ | ✅ | ✅ loopback local model |
| BehaviorGraph | ✅ | ✅ | ✅ | ✅ browser observations |
| Delta Triangle | ✅ | ✅ | ✅ default verdict axis | ✅ same-program base/head Chromium replay |
| Scoped OpenSpec authorization | ✅ | ✅ | ✅ default verdict axis | ✅ base/head requirement/scenario diff per program |
| Scoped `CodeDelta` | ✅ | ✅ | ✅ default verdict axis | ✅ obligation → flow → Weavatrix node intersection |
| Spec Recovery | ✅ | ✅ | ✅ opt-in | ✅ Git + Weavatrix, QA-gated |
| Test lineage | ✅ | ✅ | ✅ protection path | ✅ exact passing-case inventory independent of impacted coverage; aggregate coverage stays executor-level |
| ProtectionSnapshot/Delta | ✅ | ✅ | ✅ default verdict axis; base replay stays explicit | ✅ base/head coverage replay |
| Protection MCP/Studio | ✅ | ✅ | ✅ verdict axis + Studio summary; 🟡 opt-in profile/view | ✅ when measured coverage exists |
| Real Playwright TestProgram | ✅ | ✅ | ✅ | ✅ actions + exact spans + sealed assertions + observations |
| Manual record → promoted replay | ✅ | ✅ | ✅ CLI/MCP/Studio + explicit promotion | ✅ passive Chromium capture + novelty discard + replay preview |
| Deterministic network replay | ✅ | ✅ | ✅ config + run/record/CAS | ✅ Chromium record → strict replay, same profile on base/head |
| Privacy-safe request identity | ✅ | ✅ | ✅ journal + replay + duplicates | ✅ method/path/content-type/body digest; GraphQL op/query/variables hashes |
| Honest visual digest | ✅ | ✅ | ✅ default behavior axis | ✅ SHA-256 of `screenshot_png`; no perceptual kernel |
| Accessibility built-ins and continuity | ✅ | ✅ | ✅ default UI verdict | ✅ Playwright facts + base/head ratchet |
| Mutation | ✅ | ✅ | ✅ default Proof producer | ✅ changed-line Go/Vitest exact-case execution |
| Metamorphic | ✅ | ✅ primitive | ❌ project adapter | ❌ project execution |
| Cheap explorer | ✅ | ✅ planner | ❌ browser feedback | ❌ closed loop |
| Application Surface Graph | ✅ | ✅ | ✅ read-only artifact + MCP/Studio | ✅ live run persist; not a gate |
| Studio API | ✅ | ✅ | ✅ | ✅ local HTTP |
| Studio frontend | ✅ idea | ❌ | ❌ | ❌ |
| Composite `ChangeQualityVerdict` | ✅ | ✅ | ✅ default `quality_verify` | ✅ live PASS / BLOCKED / NOT_ENOUGH_EVIDENCE |
| Protection as a default verdict axis | ✅ | ✅ | ✅ | ✅ from stored base/head snapshots |
| Committed product fixture | ✅ A/B1–B5 | ✅ A/B1–B5 | ✅ CLI/MCP/Studio for B2/B4/B5 | ✅ A/B1–B5 |
| Sealed UI predicates | ✅ | ✅ | ✅ | ✅ Chromium, each asserted both ways |
| `LayoutSnapshot` v2 | ✅ | ✅ | ✅ | ✅ bounded, redacted, settle-checked |
| UI Integrity detectors | ✅ | ✅ | ✅ | ✅ Chromium base/head fixture |
| UI Integrity ratchet | ✅ | ✅ | ✅ | ✅ new/existing/fixed/returned/excepted |
| UI Integrity in MCP/Studio | ✅ | ✅ | ✅ | ✅ `quality_verify`, `quality_explain`, summary |
| React render profiler | ✅ idea | ❌ | ❌ | ❌ |
| Duplicate mutation requests | ✅ | ✅ | ✅ default UI verdict | ✅ Playwright base/head action spans |
| Responsive interval search | ✅ | ✅ | ✅ default UI verdict | ✅ Playwright base/head, exact pixel boundary |

Current implementation: the composite change verdict, deterministic UI-integrity axis, live Delta Triangle, passive recorder, runner-neutral network replay, changed-region source mutation, requirement/scenario-scoped OpenSpec authorization, and program-scoped `CodeDelta` are on `main`. Every normal browser run replays the exact head-selected `TestProgram`, seed, sealed oracles, and network profile against the merge-base runtime, stores bounded base observations, joins the measured behavior delta with scoped OpenSpec intent and Weavatrix code facts, and exposes the axis through CLI, MCP, and Studio without a separate comparison command. Mutation-enabled normal runs apply bounded TS/JS or Go edits only in a detached worktree and attach the exact test decision to the matching obligation's Proof. The current tree embeds the published `weavatrix-rust` 2.7.4 and adds a clean committed A/B1/B2/B3/B4/B5 product fixture across React/Vitest/Playwright, Node, Go, and nested OpenSpec. Exact execution evidence and merge-base proof provenance remain as `43d6e02`; cross-platform Cargo evidence hardening is `adfe53b`. The previous published committed-protection vertical (`5e2e167`) passed GitHub Actions run [32708728281](https://github.com/Weavatrix/weavatrix-quality/actions/runs/32708728281) across clean-checkout workspace, Playwright, typed JavaScript, installable-package smoke, and Clippy checks.

Accessibility now uses the same ordinary base/head browser replay as UI integrity; no opt-in view is needed to gate a change. Playwright v2 snapshots collect only bounded semantic facts: tag/type, computed role and accessible name, label association, keyboard focusability, native/ARIA disabled state, selected role states, dialog modality/focus, and whether a concrete sealed predicate target named the node. Rust owns six standards-derived checks for control name, form label, required-flow keyboard reachability, role/state consistency, dialog name, and modal focus. New defects on sealed targets block; non-required defects are warning debt, so adoption does not hand QA an unrelated cleanup queue. Old defects remain ratcheted, fixed defects are credited, missing or truncated evidence is not clean, and the full path uses zero runtime model or vision tokens. A real fixture keeps its behavioral seal `PROVEN` while removing the required Export button name; normal `run` stores the delta and composite `quality_verify` blocks it.

The old `Pixel` axis compared screenshot CAS handles, so two identical images with different handles looked like a visual change, and the name implied a perceptual kernel that does not exist. The axis is now `visual_digest`: Rust hashes the captured PNG bytes, the digest names its surface (`screenshot_png`), and comparison runs only when structured axes already matched and both sides actually have a digest. A handle without a digest is not visual evidence. A perceptual engine is a later layer, not this one. This is also the render-byte proof Weavatrix SEO should consume rather than wrapping Chromium a second time.

Request identity is method + path + content type + a canonical body digest. GraphQL is keyed by operation name, query hash, and variables hash. Raw request bodies never enter the journal, the replay profile, or the comparison token. Two POSTs to the same path with different JSON, or two GraphQL operations on `/graphql`, are different identities; key order and query whitespace do not create a false delta. Replay profiles are `schema_v: 2` and still accept v1 method/path-only documents. The canonical TypeScript source owns that v2 identity; CI rebuilds `dist/` and fails if it drifts from the committed tree.

Network replay is a separate bounded artifact, not a relaxation of ordinary observation redaction. The browser captures only same-origin fetch/XHR JSON responses; request headers, cookies, non-JSON bodies, sensitive JSON keys, email-like values, bearer tokens, and JWT-like strings never enter the profile. Normal observations still contain method, URL, resource class, optional status, and privacy-safe identity hashes — never bodies or header values. `live`, `record`, strict `replay`, and `hybrid` are parsed from repository policy with unknown fields and invalid bounds refused. Strict replay aborts an unrecorded API call and fails the run even if a sealed UI assertion passes. A real command-bus fixture captures a profile through passive recording, retrieves it from CAS, loads it through versioned `config.yaml`, and proves that both head and merge-base replay make zero additional upstream API calls.

Storybook's official Vitest addon is a distinct registered executor rather than a generic npm script. Discovery requires a Storybook config, Vitest, and the official addon; V8 coverage is requested only when the package declares the provider. The base/head Weavatrix impact union promotes affected `.stories.*` files into the safe selection, and the frozen invocation targets only the `storybook` Vitest browser project. A real React fixture executes its `Saves` play function in Playwright Chromium, emits one exact JUnit case plus LCOV for `Button.tsx`, and proves the bound `save-operates` obligation. A JUnit failure also fails the executor when the child process itself returns zero. Full declaration spans required for LCOV mapping come from the published `weavatrix-rust` 2.7.4; the upstream TSX/JS/Go regression test and multi-OS release gate are green.

Every attempted measured browser step now owns exact start/end observation indexes; setup preconditions remain outside user-intent classification. The Playwright boundary keeps a bounded, monotonic request journal of method, path, content type, optional status, and a body or GraphQL digest, settles immediate application-level retries for at most two seconds, and records no request bodies or header values. Rust classifies repeated POST/PUT/PATCH/DELETE identities only inside one action span. Two identical requests in different spans remain two user intents; a repeated mutation within one span becomes base/head-ratcheted `WVQ-UI-NET-001` and blocks the default verdict. Disabled or truncated network evidence marks the measurement incomplete rather than clean. A real Chromium fixture proves a response-triggered POST retry is caught while the same sealed behavioral oracle still passes.

Responsive measurements set the actual Playwright viewport through the Rust-owned browser protocol. The browser collects bounded breakpoint hints from parsed media rules, stylesheet media attributes, and container rules; Rust probes each boundary and its neighbours, bisects only observed base/head state transitions to one CSS pixel, and carries the measured interval into the ordinary composite verdict. A real fixture stays clean at the default 1280×720 viewport, moves a control outside the viewport at a `width < 768px` rule, reports the exact 320–767 px failure interval, and blocks `quality_verify`. A transient incomplete browser observation is repeated once at the same width; a second incomplete result remains incomplete and fails closed. Incomplete stylesheet access or a spent probe budget fails to `unmeasured` instead of becoming a clean result. No runtime model or vision tokens are used.

Current validation: 96 `wvq-proof` tests, 34 `wvq-spec` tests, 73 `wvq-ui` tests, the 68-test runtime suite, 44 command-bus library tests, 36 general command-bus integration tests, 3 real source-mutation scenarios, all 12 real Chromium UI/Storybook scenarios, and 24 Playwright-runner tests pass. The mutation scenarios prove a surviving Go boundary mutant weakens an otherwise green high-risk proof, a boundary-specific Go case kills it and remains `PROVEN`, and one exact Vitest case judges a changed JavaScript line while an unbound case in the same file cannot claim the kill. The user's source stays byte-identical. One browser scenario proves a response-triggered POST retry is captured inside its originating action span. Another records a redacted API response, replays it without calling upstream, and fails closed on an unrecorded strict request. A third changes accessible behavior and a Weavatrix-visible TypeScript function without changing OpenSpec: the sealed assertion remains `PROVEN`, but an ordinary `run` persists `WVQ-BEHAV-001` and the default composite verdict is `BLOCKED` without calling an opt-in view. A fourth removes the accessible name from a sealed Export target: the functional oracle still passes, but requirement-aware `WVQ-A11Y-NAME-001` blocks the ordinary composite verdict. A fifth keeps two requirements in one change folder and edits only the `Theme` requirement while a `Checkout`-bound program drifts: the unrelated OpenSpec edit does not authorize it, the reading stays `unintended_behavior_drift`, and `quality_verify` returns `BLOCKED`. A sixth changes `src/theme.ts` while checkout behavior drifts: the theme nodes are not in the checkout program's protected surface, so the code axis stays false and `WVQ-BEHAV-001` does not fire. Warnings-denied Clippy covers all targets in `wvq-runtime`, `wvq-proof`, and `wvq-command-bus` plus their transitive WVQ crates.

## Changed-region source mutation

Mutation is part of ordinary `quality_run` and `quality_verify` whenever a scenario declares mutation hints. Rust plans concrete, content-derived edits only on Git-changed lines in non-test TS/JS or Go sources. The TS/JS catalogue covers boundary/equality/boolean/logical/off-by-one changes, branch/sort/permission/callback/error omissions, and collection boundaries; the Go catalogue covers error/nil and other boundaries, zero returns, skipped branches, ignored context, and inverted booleans. Strings, comments, TSX markup-shaped comparisons, Go `if` initializers, property calls, and unrelated same-line comparisons are excluded by concrete rules rather than confidence scoring.

Execution creates a detached worktree at the exact head commit, overlays the requested working tree, links only an existing package dependency directory, applies one mutation, and invokes a frozen executor with the exact policy-bound case. The supported measured adapters are Go test and Vitest/Storybook-Vitest. Dependency links are explicitly unlinked before the temporary worktree is removed; the user checkout is never edited. Compile errors and missing/ambiguous normalized cases are `invalid`, not falsely `killed`. Known flaky judges — `flake_penalty` in `quality.yaml` or historical pass+fail on the exact case — are excluded: a flake cannot independently produce `killed` and therefore cannot strengthen Proof. One mutant is capped at 120 seconds and 2 MiB of output; the whole phase is capped at 600 seconds, 32 source edits, and 128 obligation-case decisions. Any applicable obligation not reached by a cap remains `unmeasured` even if another obligation was measured.

The `mutation-results` artifact is schema-validated against the current `quality.yaml`: obligation set, applicable subset, operator/ecosystem authorization, source region, unique result identity, counters, global state, and zero-token invariant must agree. Each result names the exact obligation and normalized test identity that judged it. Survived, invalid, and required-but-absent evidence weaken an otherwise green Proof to `PARTIAL`; a killed mutant for one obligation cannot strengthen a different obligation. Custom project-semantic hints remain listed as unmapped limitations instead of being guessed into unsafe source edits.

## Composite change verdict

`quality_verify` no longer aggregates `ProofVerdict` alone. Proof, protection, debt, stability, AI budget, UI integrity, and Delta Triangle are separate axes joined by a fixed priority order; each keeps its own facts and provenance and there is no opaque score. `blocking` and the process exit code follow the composite state, while the `verdict` token stays backward compatible.

Delta Triangle is now produced on the ordinary browser execution path. The head-selected program is the authority for both sides; the base checkout contributes only its versioned runtime coordinates, so a moved or changed test cannot compare itself with a different base program. Preview origins are removed from network identity before comparison. Structured axes are compared before pixels across every observation, base evidence stays behind CAS handles, an incomplete side is `unmeasured`, and `WVQ-BEHAV-001` blocks when Weavatrix sees code change and runtime behavior changes without an authorizing OpenSpec delta. This path uses zero model and vision tokens.

## Scoped OpenSpec authorization

The spec axis is no longer one boolean for the whole change folder. `wvq-spec` reads the same change at the merge base, builds a requirement/scenario snapshot of both revisions, and returns the exact `SpecChangeScope` that changed. A changed requirement operation, name, or normative body authorizes every scenario under that requirement and nothing else. A changed scenario authorizes only itself. A change folder absent at base authorizes every requirement it declares. Source locations are excluded on purpose: moving unchanged prose is not an intent change. Mismatched change ids and ambiguous duplicate requirement operations fail closed.

`wvq-proof` turns that scope into a per-program decision. A program is spec-authorized only when *every* obligation it asserts lies inside the changed scope; one matching obligation cannot authorize a mixed program, and a program that asserts no obligation is never authorized. Unit coverage asserts each of those rules directly, plus the three fail-closed paths: relocated but unchanged prose produces an empty scope, mismatched change ids are refused, and duplicate requirement operations are refused rather than silently deduplicated. The `delta-triangle` artifact is now `schema_v: 3` and records `spec_authorized`, `authorized_obligations`, `unauthorized_obligations`, `code_measured`, `code_changed`, `code_nodes`, and `code_unmeasured_reason` per program; v1 and v2 documents stay readable.

This closes the concrete soundness hole where changing requirement A excused unrelated behavior drift in a program bound to requirement B, even though both live in the same OpenSpec change and the same file was touched. A real Chromium fixture proves it: one repository, one change folder, two requirements, a Weavatrix-visible TypeScript edit, and an OpenSpec edit that touches only `Theme` — the `Checkout` program's drift stays `unintended_behavior_drift` and `quality_verify` returns `BLOCKED`.

## Scoped CodeDelta

The code axis is no longer one boolean for the whole repository. `graph_diff` still supplies the changed Weavatrix node ids (added, removed, changed before/after, and edge endpoints), but that set is not copied onto every program. For each program, `scoped_code_delta` intersects the program's obligations with the flows that proved them — coverage-measured `FlowProtection` plus declared `test_bindings` that name a source file Weavatrix actually graphed — and with those changed node ids.

A nonempty intersection is a measured `true` and lists the intersecting nodes. An empty intersection is a measured `false`. A program that asserts no obligation, or whose obligations have no flow that names Weavatrix nodes, is `unmeasured` with an explicit reason; it never inherits the repository-wide `graph_diff` bit. Missing mapping is attribution evidence, not a missing replay: Spec × Behavior still decides authorization. Unchanged behavior with unmeasured code stays `NoChange` and does not fire `WVQ-VERDICT-007`. Unauthorized behavior drift still blocks as `WVQ-BEHAV-001`. The artifact records `code_unmeasured_programs` as a limitation, not as an unmeasured axis. `ObligationCodeSurface` is the shared mapping: implementation Weavatrix nodes only. Test, spec, and Storybook nodes stay on the test side and cannot make `CodeDelta` true. Mutation uses the same surface so a payment mutant is not judged by a pagination obligation when both have declared implementation paths.

This closes the matching hole on the code axis: a `theme.ts` Weavatrix node cannot satisfy a checkout program bound to `src/app.ts`. A real Chromium fixture proves both sides. Changing `src/app.ts` while checkout behavior drifts still yields `unintended_behavior_drift` and `WVQ-BEHAV-001`. Changing only `src/theme.ts` while the same checkout behavior drifts leaves `code_changed` false and does not fire `WVQ-BEHAV-001`.

An axis reports `not_applicable` when the change has no surface it can measure and `unmeasured` when it has that surface and the evidence is absent. The distinction is the point: missing evidence never becomes a pass, and an axis that was never in scope is not reported as a gap.

Priority order, most important first: an active sealed-oracle contradiction; lost critical protection; new blocking architecture, API, or security debt, or a new blocking UI regression; a mandatory obligation left unproven; the same two classes returned after being fixed; a mandatory test with an unresolved new flake or an ambiguous specification; a required axis that was not measured; an AI budget exhausted with a mandatory decision still open; warning-only drift. The first rule that fires decides the state; every other fired rule stays listed.

Three invariants are asserted rather than assumed: a `PROVEN` behavioural proof cannot suppress a lost protection delta, a global coverage gain cannot suppress a local protection loss, and missing evidence is never a pass. Live fixtures cover all three outcomes — a healthy Cargo change composes to `PASS`, a change that deletes the only test reaching `subtract` is `BLOCKED` while its suite is green, and head coverage with no base snapshot is `NOT_ENOUGH_EVIDENCE` at exit code 1.

Protection is a default axis. `quality_verify` composes it from the head snapshot every run already persists plus the base snapshot `protection_view` now stores against that run, so no caller attaches a `ProtectionView` and `verify` still executes nothing.

## Committed monorepo protection fixture

The product fixture creates a real temporary Git monorepo with two clean commits and a clean worktree. It contains a React/Vitest frontend with V8 LCOV, a Node backend, a Go service with `go test -json` plus a coverprofile, a Playwright `TestProgram` executed in Chromium, and nested OpenSpec plus `quality.yaml`. All evidence is produced through registered executors; no fixture inserts a synthetic `ProtectionSnapshot`.

- **A → B1, healthy refactor:** implementation and its exact protector move files and the function moves to a different source line. Symbol identity is relocated without using the old path or source position, measured protection is preserved, and the composite verdict passes.
- **A → B2, phantom protector:** the exact Go case still passes but no longer invokes any product function. Its revision-bound normalized identity is retained independently of impacted coverage, so WVQ reports a surviving phantom rather than a deleted test. The optional protection view emits `WVQ-PROTECT-002`, and the same stored loss blocks ordinary `quality_verify`, actual MCP JSON-RPC, and the Studio summary while behavioural proof remains `PROVEN`.
- **A → B3, deleted protector:** the sole exact protector disappears while the remaining suite is green. Both the detailed view and default verdict retain `WVQ-PROTECT-003` and block.
- **A → B4, intended expectation replacement:** OpenSpec, its compiled obligations, the Go protector, and the Playwright assertion change from viewer denial to viewer allowance. WVQ stores one immutable proposal containing base/head/merge-base, the exact Weavatrix content revision, both full seal digests, and the explicit obligation mapping. A stale digest or developer acceptance cannot authorize it. One exact QA or product-owner acceptance makes the old helper paths obsolete, the new protector `REPLACED`, and CLI, MCP, and Studio return `PROVEN` with a non-blocking composite state and zero runtime model tokens.

Coverage is assigned to an exact case only when normalized evidence contains one passing case and its runner, suite, and case match a repository binding. Multi-case coverage remains executor-level evidence; WVQ does not guess which case reached a symbol. Separately, every passing normalized case is stored in the revision snapshot even when it reaches no impacted flow. This separation lets lineage distinguish a deleted protector from a surviving phantom without weakening coverage attribution. Snapshots written before the inventory remain readable and derive their known executed identities from measured flow protectors. Current Weavatrix nested spans are read at symbol granularity, and the file node is used only as a fallback when that source has no symbol spans.

**A → B5, missing declared intent:** the Go `CanDelete` implementation and its exact protector change while nested OpenSpec does not. Weavatrix 2.7.4 identifies the concrete exported function from a full declaration fingerprint; WVQ filters private helpers and test declarations, prepares one bounded candidate, and exposes the same `QA_REVIEW` state through `wvq recover`, the opt-in MCP recovery profile, and Studio. The changed implementation plus its own test is marked as a weak oracle, requires intent-owner escalation, spends zero model tokens, and cannot auto-seal. A real OpenSpec delta suppresses this recovery candidate instead of asking for redundant review.

## UI integrity

`wvq-ui` is a pure-Rust crate: no browser, no DOM, no model. Collection stays in `js/playwright-runner`, orchestration in `wvq-command-bus`, evidence in `wvq-store`, sealed expectations in `wvq-spec`, so detector logic exists exactly once.

`LayoutSnapshot` v2 is deliberately not a DOM. It carries geometry, semantic identity, bounded accessibility facts, and hit-test results — never `innerHTML`, form values, cookies, storage contents, response bodies, or unbounded text. Labels are collapsed and cut to 120 characters in the page before they leave it, input values are never used as names except for button-like input types, and `textContent` only names an element that is a control or a leaf, so a list row never copies the text of everything inside it.

Collection is deterministic or it says so. Fonts are awaited with a bounded timeout, animations and transitions are frozen and driven to their end state, and the page is read twice: two reads that disagree beyond tolerance mark the snapshot unsettled instead of trusting one of them. Screenshot capture for `VisualDigest` uses the same freeze: webfonts are awaited, CSS time is stopped, the caret is hidden, and Playwright records with `animations: disabled` so a blinking caret cannot change the PNG hash. Region-guided visual diff then names the impacted surface: nodes are paired by semantic identity, clipped rectangles become crops, and exact RGBA is compared only inside those crops. A chrome pixel cannot attribute the checkout button. There is no perceptual kernel and no vision call. The collector enters open shadow roots and same-origin iframes, maps iframe geometry into the top viewport, and records a cross-origin iframe as an opaque surface rather than pretending it was empty. `clip_rect` is the intersection of every clipping ancestor, including the iframe box, not the first overflow parent. Geometry and hit testing happen in a single `evaluate`, so both describe one DOM state and node identities are derived once. Node, hit-test-sample, candidate-pair, artifact-byte, and label bounds are all explicit, and hitting any of them sets `truncated`, which propagates into the verdict as a limitation.

Axe-core and Storybook a11y are an import adapter, not a Rust port: if the page already loaded a producer, the collector keeps rule id, impact, and bounded selectors, drops HTML, and `wvq-ui` turns that into `WVQ-A11Y-IMPORT-001` findings. Impact maps to objective severity (`critical`/`serious` → error); the ratchet still decides whether the PR owns it (`new` blocks, `existing` does not). Fourteen built-in detectors ship. The UI set is `WVQ-UI-DUP-001` duplicate DOM identity, `WVQ-UI-DUP-002` duplicate test identity, `WVQ-UI-DUP-003` ambiguous interactive identity, `WVQ-UI-LAYOUT-001` interactive occlusion, `WVQ-UI-LAYOUT-002` viewport overflow, `WVQ-UI-LAYOUT-003` text clipping, `WVQ-UI-LAYOUT-004` confirmed control overlap, and `WVQ-UI-NET-001` a repeated mutating request inside one exact action span. The accessibility set is `WVQ-A11Y-NAME-001`, `WVQ-A11Y-LABEL-001`, `WVQ-A11Y-KEYBOARD-001`, `WVQ-A11Y-STATE-001`, `WVQ-A11Y-DIALOG-001`, and `WVQ-A11Y-DIALOG-002`.

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

Bounded runner and Playwright-bridge spawns sit in a Unix process group or a Windows job object. Deadline, cancel, and output-cap kills terminate Vitest workers and Chromium descendants, not only the parent `Child`. A Node grandchild that keeps writing after the parent is killed is the regression test.

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
| record | Opens a visible Playwright browser by default, captures bounded semantic natural use plus a separately redacted same-origin JSON network profile, never exports unmatched form values, evaluates existing sealed predicates at the exact final state, computes new states/non-loop edges/API/obligation links, discards zero-contribution sessions, and replays useful `source: recorded` candidates through preview with zero model tokens |
| promote | Revalidates the exact previewed program, current repository revision, change, and existing `OracleSeal`; only a passing preview atomically becomes CAS-backed program revision 1, and repeated promotion is idempotent |
| reuse | `select` and `run` automatically load the latest promoted revision whose seal still matches; stale-seal programs are not executed, and a repository-configured program cannot silently shadow the same id |
| heal | Accepts only semantic retargeting or typed deterministic waits, requires the caller's latest program revision and the same `OracleSeal`, runs real Playwright with the original assertions, and atomically appends a CAS-backed revision only on pass; failed repairs retain evidence but do not replace the active program |
| transports | CLI: `wvq record`; MCP: `quality_test_{draft,validate,preview,promote,record,heal}`; HTTP: `POST /api/v1/authoring/{draft,validate,preview,promote,record,heal}` |

Affected-package validation: 107 tests passed with zero failures, including the real Rust → stdio bridge → Playwright preview with two screenshots and a trace. Clippy passed for `wvq-runtime`, `wvq-command-bus`, `wvq-mcp`, and `qualityd`, all targets, with warnings denied.

Current validation: 493 Rust tests pass with zero failures, and workspace Clippy passes for all targets with warnings denied. The prior 20 Playwright-runner tests, 5 JS package tests, strict public `NodeNext` declarations, and corroborated real shadow benchmark remain unchanged by the B4 path.

## JS/npm distribution

- Package name: `wvq`; the JS API is a typed, no-shell boundary over Rust rather than a second policy implementation.
- `npx wvq`, `npx wvq mcp`, and `npx wvq bench` select only the three fixed native programs. Direct `wvq-mcp` and `wvq-bench` bins are also present after installation.
- `WvqClient` covers the CLI command bus, including passive `record`; `WvqMcpClient` provides generic bounded calls plus typed authoring `draft`, `validate`, `preview`, `promote`, `record`, and `heal` helpers.
- The package resolves bundled Windows/macOS/Linux x64/arm64 programs, verified platform packages, explicit binary overrides, or the local workspace binaries. It never falls back through a shell or recursively launches itself from `PATH`.
- Ordinary CI stays a full Linux workspace. A second smoke job runs process-tree kill, executor limits, path canonicalization, and `wvq`/`wvq-mcp` compile on `windows-latest` and `macos-latest` so Job Objects and non-POSIX paths are not Linux-only assumptions. The tag workflow still builds and smoke-tests all three programs on six platform runners, assembles the universal package, installs it into a clean prefix, exercises a real MCP call, publishes npm with provenance, validates/publishes official MCP metadata, verifies both registries, and then creates an immutable GitHub release.
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
| network replay | Captures bounded redacted same-origin JSON response profiles into CAS, loads versioned repository profiles for strict/hybrid replay, and applies the exact head profile to both head and merge-base browser execution |
| delta triangle | Replays each exact head-selected `TestProgram` against the merge-base runtime, compares every structured observation before pixel evidence, joins Git/OpenSpec + Weavatrix + behavior, and persists the default verdict axis |
| source mutation | Plans bounded built-in edits on changed TS/JS or Go lines, executes exact obligation-bound Go/Vitest cases in an isolated Git worktree, and feeds per-obligation killed/survived/invalid evidence into Proof |
| ui integrity | Collects one bounded, settle-checked layout snapshot per step, runs the detectors in Rust, and persists `ui-layout-snapshot`, `ui-hit-test-map`, and `ui-integrity-findings`; `ui_integrity_view` replays the base and stores the classified `ui-integrity-delta` |
| proof | Uses only the latest same-change, same-revision run; proof requires an exact normalized runner case or browser assertion, and persists the linked revision/evidence artifacts |
| expectation replacement | Compares base/head `OracleSeal` documents, persists one immutable revision-bound proposal, refuses stale or developer-only approval, and feeds an exact QA/product-owner acceptance into the default protection verdict |
| debt | Uses immutable base/head Weavatrix evidence and persistent fixed-history to classify `new/existing/fixed/returned/excepted` |
| AI | Explicit opt-in loopback completion path, preflight reservation, server usage evidence, global + change-local ceilings, persistent per-change spend |
| verdict | Composes proof, protection, debt, stability, AI, UI integrity, and Delta Triangle into one ranked change-level state from stored evidence, without executing anything |

`plan` reads existing same-revision proofs. `explain` resolves obligations, proofs, runs, selections, debt findings, and UI-integrity findings with provenance; a UI explanation names the target, the occluding or duplicate counterpart, the route and viewport, the exact probe and geometry numbers, and the artifact handles. `status`, evidence handles, proofs, debt history, and AI usage survive a new process.

## Safety invariants exercised

- no arbitrary shell over MCP;
- large artifacts remain handles;
- unknown schema versions, command values, stale/malformed evidence, revision drift, and incomplete graph diffs fail closed;
- missing coverage is unmeasured, never uncovered;
- a compile error or missing/ambiguous exact case cannot kill a mutant, and mutation strength never crosses obligation boundaries;
- a successful unbound suite remains `UNPROVEN`;
- normal verification makes no model call and spends zero runtime tokens;
- model calls accept loopback HTTP only and are refused before network I/O when budget cannot cover the reservation;
- detector blocking requires `High` weight and per-signal `Confirmed` graph corroboration;
- an axis with no surface is `not_applicable` and an axis with no evidence is `unmeasured`; neither is reported as clean;
- a truncated or unsettled layout snapshot is never a clean measurement;
- the UI policy refuses unknown fields, empty matchers, path-shaped values, out-of-range ratios, malformed dates, exceptions without a reason, and any `accept_all`;
- every sealed predicate must be executable in the browser, enforced by a parity test over all 24 variants;
- UI collection persists no raw markup, form values, cookies, storage contents, or unbounded text.
- network profiles persist no request headers, cookies, non-JSON bodies, configured sensitive keys, email-like values, bearer tokens, or JWT-like strings; strict replay fails on an unknown API request.

## Measured detector calibration

On sixty accepted, defect-free changes, text matching fired on 33–92% depending on category and the initial policy would have blocked 42% of clean changes. Graph-backed default-flip and retired-persisted-key categories fired on 5–8%. The graph promotes only the signal whose concrete symbol it names; `TestMovedWithImplementation` is never promoted.

## Repository maintenance debt

- `cargo fmt --all -- --check` has roughly forty pre-existing formatting differences. Repo-wide formatting remains deferred so unrelated churn is not mixed into the release; touched code passes workspace Clippy with warnings denied.
- `[profile.dev] debug = "line-tables-only"` remains in the workspace manifest to keep local build artifacts bounded.
- `impacted_story_runs_in_real_storybook_vitest_browser_mode` currently fails on this development machine, identically on `main` and on the working tree (same assertion, 901 s both times), and leaves an orphaned Vitest browser-mode process behind. It is an environment failure, not a regression: the Storybook fixture declares no `browser:` section, so the base browser replay and Delta Triangle code never execute for it. CI covers this test; a local run should kill leftover `vitest.mjs` processes from a previous attempt before retrying.

## Load next

Correctness of the existing axes comes before any new feature family. In order:

1. **Surface Evidence Matrix, gap classification, cheapest-evidence planner, observe-only calibration.** Coverage Autopilot closed-loop generation waits until those exist. The Application Surface Graph is now a read-only persisted projection, not a planner.
2. **Bounded failure evidence** is already a library + bridge path (`failure_reel`); keep it diagnostic-only and never a verdict source.
3. **Then breadth.** Continuous recorder package, extended cassette, Studio frontend, `wvq baseline` with `OBSERVED_ONLY`, and remaining first-class browser actions (upload/download/popup/tab). `wvq init` and hover/scroll/drag are already on `main`.
4. **Advanced producers.** Project metamorphic adapters and the browser-feedback exploration loop.

Do not duplicate Rust policy or proof semantics in TypeScript, and do not add a default MCP tool for UI detail — `quality_verify`, `quality_explain`, and `quality_evidence` already carry it.
