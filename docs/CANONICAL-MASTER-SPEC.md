# Weavatrix Quality — CANONICAL MASTER SPEC & DEVELOPMENT PLAN


> **Canonical status — 2026-08-18:** This document supersedes the earlier `weavatrix-quality` drafts and consolidates the full design discussion: the initial large GitHub/PyPI/npm testing-landscape research, the OpenSpec integration, Rust/Weavatrix architecture, low-token browser strategy, Quality Debt Ratchet, Spec Recovery, dual-revision impact, Coverage/Protection Continuity, record/replay, Proof model, mutation/metamorphic validation, flake/healing, MCP/CLI/Studio boundaries, and the final phased development plan.
>
> **Conflict rule:** when an older idea conflicts with a later section, the later, stricter decision wins. In particular: no generic browser-MCP clone, no head-only coverage logic, no implementation-derived oracle without QA verification, and no LLM in the normal execution path.


> **For agentic workers:** REQUIRED SUB-SKILL: use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. All implementation work should follow TDD and end each independently reviewable task with a commit.

**Goal:** Build `weavatrix-quality` (WVQ), a Rust-first Spec-to-Proof quality platform that turns OpenSpec intent, Weavatrix revision-bound code evidence, deterministic test execution, runtime behavior and historical quality evidence into minimal-cost, revision-bound verification with exception-only human QA.

**Architecture:** WVQ is a separate product that embeds `weavatrix-rust` as its code-intelligence engine, consumes OpenSpec as the human-readable intent authority, compiles quality obligations into a typed `TestProgram` IR, orchestrates existing JS/TS/Bun/Go/browser runtimes, and stores immutable `Proof` records. The normal green path is deterministic and model-less; AI is restricted to planning, semantic ambiguity, novel exploration and difficult failure classification.

**Tech stack:** Rust 1.89+; `weavatrix-rust`; `mcport`; SQLite; content-addressed artifact storage; a thin TypeScript Playwright bridge; Playwright; Vitest/Jest/Bun test; Go `go test -json`/coverage/race/fuzz; OpenSpec compatibility reader; CLI + HTTP + MCP.

**Normative spec:** Sections 1–35 of this document.  
**Implementation plan:** Sections 36–58.

## Global constraints

- Rust owns quality policy, deltas, risk, selection, evidence, proof, storage, budgets and MCP/HTTP semantics.
- `weavatrix-rust` is the authority for repository/code facts. WVQ must not create a second code graph.
- OpenSpec is the authority for intended externally visible behavior. WVQ consumes it; WVQ does not fork or replace OpenSpec.
- Playwright remains the browser engine. WVQ does not build a browser.
- TypeScript/JavaScript is allowed on the runtime boundary where existing tools are strategically superior; decision logic stays in Rust.
- Routine green-path verification must use **0 runtime LLM tokens**.
- Existing test replay, graph diff, code-health checks, mutation execution, behavior comparison and Proof assembly must be deterministic.
- No arbitrary shell command is accepted over MCP.
- Large artifacts are returned by handles; screenshots/HAR/video are not dumped into an LLM context by default.
- AI-generated repair may never silently change a sealed business expectation to make a failing test pass.
- Existing quality debt can be baselined. New debt is separately classified and can block; old debt does not force a repository-wide cleanup before adoption.
- The first-class result is `Proof`, not a test file and not a single “quality %”.
- Every result must carry repository/revision provenance.
- JS/TS/Node/Bun and Go are first-class v1 target ecosystems.
- Missing evidence is never interpreted as evidence of absence.
- Fail closed on unknown schema versions, unknown quality actions, invalid evidence, ambiguous revision identity and attempts to mutate a sealed oracle.

---

# Part I — Product and architecture specification

## 1. Product thesis

Once PMs and developers use strong coding agents, production of change can become much faster than a small QA team can manually understand, exercise, triage, retest and regress.

WVQ changes the workflow from:

```text
read task
→ invent cases
→ manually navigate
→ broad regression
→ inspect failures
→ write bug
→ wait for fix
→ retest
→ repeat
```

to:

```text
OpenSpec intent
→ sealed test obligations
→ Weavatrix code/graph delta
→ minimal deterministic execution
→ revision-bound Proof
→ human sees only unresolved exceptions
```

The goal is not “a tester clicks ten times faster”. The goal is to make most routine clicking, regression, evidence gathering, triage and retesting no longer require a human.

### Product metrics

WVQ should measure, not assume:

- `human_QA_minutes / PR`;
- `manual_retests / PR`;
- `manual_triage_minutes / failure`;
- `selected_tests / full_suite_tests`;
- `selected_suite_wall_clock / full_suite_wall_clock`;
- escaped regressions;
- false-positive quality findings;
- flake rate;
- mutation sensitivity;
- QA AI tokens per PR;
- QA AI tokens / development AI tokens;
- browser AI escape calls;
- vision calls;
- artifact bytes.

Initial design targets:

```text
routine regression runtime LLM tokens = 0
QA AI tokens <= 20% of development AI tokens when measurable
eligible routine human touch time reduction ≈ 10× target
escaped regressions must not increase
```

These are benchmark targets, not unmeasured product claims.

---

## 2. Research principles baked into the design

### 2.1 Plan-then-execute instead of ReAct-by-default

The 2026 paper *Web Agents Should Adopt the Plan-Then-Execute Paradigm* argues for compiling a task-specific program and then executing it. Its WebArena analysis reports that 80% of tasks can use a purely programmatic plan without runtime LLM subroutines.

WVQ makes this an architectural invariant:

```text
OpenSpec / human / agent
        │
        │ compile rarely
        ▼
    TestProgram
        │
        │ execute repeatedly
        ▼
Rust + existing runner
        │
        └── 0 LLM tokens on normal runs
```

### 2.2 Persist behavior memory instead of repeatedly re-reasoning from screenshots

ActionEngine (2026) shows the value of state-machine memory and programmatic execution rather than screenshot→LLM→action loops. WVQ generalizes this into a persistent `BehaviorGraph`.

### 2.3 Project/testing knowledge should be structured

KTester (ICSE 2026) separates test-case design from method generation and injects project/testing knowledge. WVQ therefore ships deterministic quality heuristics and uses Weavatrix evidence instead of relying on prompts such as “think like a senior QA”.

### 2.4 Static + dynamic evidence is stronger than prompt dumping

Panta (ICSE 2026) combines static control-flow analysis with dynamic coverage to guide test generation. WVQ combines Weavatrix graph evidence with measured runtime coverage and BehaviorGraph evidence.

### 2.5 Production-scale test generation needs cost engineering and deterministic scaffolding

Uber AutoCover (ICSE-SEIP 2026) combines relevant context retrieval, test scaffolding, execution, coverage deltas, mutation/branch validation and flake defenses. WVQ adopts these lessons but pushes LLMs farther away from normal runtime.

### 2.6 Test oracles must be independent from faulty implementation

The 2026 study *On the risk of coding before testing* reports fault-detection degradation when tests are generated after exposure to faulty code. WVQ therefore introduces `OracleSeal`: expected behavior is derived from OpenSpec and sealed independently of implementation repair.

### 2.7 Coverage is not Proof strength

SWE-Mutation (2026) shows that LLM-generated suites can appear plausible while missing realistic defects. WVQ therefore keeps separate evidence for:

```text
execution coverage
requirement/scenario coverage
behavior coverage
mutation sensitivity
final Proof verdict
```

### 2.8 Flake repair needs selective code context

FlakyGuard uses graph-selected context and reports industrial repair gains. WVQ first performs deterministic fingerprinting/classification and asks an agent only for a bounded graph-grounded decision when deterministic diagnosis is insufficient.

---

## 3. Authority boundaries

### OpenSpec owns

```text
what SHOULD be true
requirements
scenarios
behavioral deltas
human/product intent
```

### Weavatrix owns

```text
what EXISTS in source
revision-bound graph
symbols/dependencies
graph diff
change impact
APIs/transports
coverage attachment
history/co-change
architecture evidence
duplicates
dead-code candidates
hot paths
```

The current `weavatrix-rust` engine already supports JavaScript/TypeScript, Go, HTTP/GraphQL/gRPC/event surfaces, Git/history, `graph_diff`, `change_impact`, `select_tests`, `coverage_map`, `find_dead_code`, `find_duplicates`, `run_audit`, `hot_path_review`, API tracing and Architecture Firewall. WVQ consumes those primitives.

### Weavatrix Quality owns

```text
what MUST be proven for this change
spec ↔ code ↔ behavior traceability
quality debt ratchet
risk
test obligations
OracleSeal
TestProgram
runner orchestration
BehaviorGraph
record/replay
Evidence Ledger
Proof
mutation/metamorphic strength
flake fingerprinting
minimal regression selection
AI Cost Firewall
quality verdict
human exception workflow
```

### Playwright owns

```text
browser process
browser/web platform execution
DOM/a11y/network primitives
trace/video/screenshot primitives
```

### Cortex Loom may optionally own

```text
context compression for agent decisions
model routing
QA decision sequences
```

WVQ CI must work without Cortex.

---

## 4. High-level architecture

```text
                    PM / Product / Agent
                           │
                           ▼
                      OpenSpec
                  requirements/scenarios
                           │
                           ▼
                      wvq-spec
             QualityContract + OracleSeal
                           │
                           ▼
                       IntentGraph
                           │
              ┌────────────┴────────────┐
              │                         │
              ▼                         ▼
       weavatrix-rust              quality history
     CodeGraph + CodeDelta        previous Proofs
              │                         │
              └────────────┬────────────┘
                           ▼
                   wvq-intelligence
          impact + risk + code-health delta
               + minimal test selection
                           │
                           ▼
                      TestProgram
                           │
             ┌─────────────┼─────────────┐
             ▼             ▼             ▼
          browser       JS/TS/Bun         Go
         Playwright       runners        go test
             │             │             │
             └─────────────┼─────────────┘
                           ▼
                      Observations
                           │
                           ▼
                    BehaviorGraph
                           │
                           ▼
                    BehaviorDelta
                           │
                           ▼
                     DeltaTriangle
                           │
                           ▼
              EvidenceLedger + CAS
                           │
                           ▼
                          Proof
                           │
             ┌─────────────┼────────────┐
             ▼             ▼            ▼
          mutation        flake       differential
             │             │            │
             └─────────────┼────────────┘
                           ▼
                     QualityVerdict
                           │
             ┌─────────────┼────────────┐
             ▼             ▼            ▼
            CLI           MCP        Quality Studio
                                      exceptions only
```

---

## 5. Three graphs and the Delta Triangle

### IntentGraph

Nodes:

```text
Change
Requirement
Scenario
RiskEvidence
TestObligation
OracleSeal
```

Edges:

```text
Change MODIFIES Requirement
Requirement HAS Scenario
Scenario REQUIRES TestObligation
TestObligation HAS_RISK RiskEvidence
OracleSeal SEALS TestObligation
```

### CodeGraph

Owned exclusively by `weavatrix-rust`. WVQ stores graph references/revisions, not a duplicate graph.

Primary operations WVQ should use:

```text
graph_stats
graph_diff
change_impact
get_dependents
select_tests
coverage_map
find_dead_code
find_duplicates
run_audit
hot_path_review
god_nodes
module_map / communities
trace_endpoint
trace_api_contract
verify_architecture
prepare_change
verified_change
git_history
cross_repo_git
memory_context
```

### BehaviorGraph

Runtime state is normalized semantically.

Example:

```yaml
route: /analytics/dashboard/42
actor: admin
component: sankey
modal: closed
network_phase: idle
data_class: above_visual_limit
feature_flags:
  new_sankey: true
```

Action:

```yaml
kind: activate
target:
  role: button
  accessible_name: Others
```

Transition:

```text
State A --activate(Others)--> State B
```

### Delta Triangle

```text
SpecDelta      what SHOULD change
CodeDelta      what code DID change
BehaviorDelta  what runtime ACTUALLY changed
```

Interpretation matrix:

| Spec | Code | Behavior | Meaning |
|---|---|---|---|
| yes | yes | yes | expected change candidate |
| no | yes | yes | suspicious unintended behavior drift |
| yes | yes | no | likely incomplete implementation |
| yes | no | no | requirement has no implementation evidence |
| no | yes | no | probable internal refactor |
| no | no | yes | environment/runtime/external nondeterminism |
| yes | no | yes | config/external path or stale code evidence; explain |

This table is evidence, not an automatic one-axis verdict.

---

## 6. OpenSpec integration

WVQ reads:

```text
openspec/specs/**
openspec/changes/**
openspec/config.yaml
openspec/schemas/**
.openspec.yaml
```

It extracts:

- change identity;
- ADDED/MODIFIED/REMOVED/RENAMED requirements;
- normative text;
- scenarios;
- GIVEN/WHEN/THEN clauses;
- capability path;
- exact source locations.

WVQ does not reimplement OpenSpec authoring UX.

### `quality.yaml`

Each behavioral change can include:

```text
openspec/changes/<change>/quality.yaml
```

Example:

```yaml
quality_contract_v: 1
change: sankey-others

risk:
  default: high

requirements:
  - capability: sankey
    requirement: visual-limit-others
    scenarios:
      - scenario: overflow-grouped

        actors:
          include: [admin, viewer]

        dimensions:
          visual_limit:
            values: [1, 10, 100]
          cardinality:
            classes: [below_limit, exact_limit, above_limit]

        obligations:
          - id: others-visible
            kind: behavioral
          - id: overflow-grouped
            kind: invariant
          - id: others-count
            kind: invariant
          - id: others-details-api
            kind: api

        evidence:
          required: [dom, network]
          on_failure: [screenshot, trace]

        mutation:
          operators:
            - boundary_flip
            - omit_group
            - wrong_count

ai:
  planning_tokens: 8000
  runtime_tokens: 0
```

An external agent can ask `quality_context(purpose="spec")`; WVQ returns neighboring requirements, current routes/components/APIs, historical regressions, quality heuristics and existing coverage. The agent drafts OpenSpec + `quality.yaml`; WVQ validates and seals it.

---

## 7. TestObligation

A `TestObligation` is more fundamental than a test file.

```rust
pub struct TestObligation {
    pub id: ObligationId,
    pub scenario: ScenarioRef,
    pub kind: ObligationKind,
    pub condition: Predicate,
    pub expected: Predicate,
    pub required_evidence: Vec<EvidenceKind>,
    pub risk: RiskLevel,
    pub oracle_seal: OracleSealId,
}
```

Kinds:

```text
behavioral
invariant
api
contract
permission
accessibility
visual
performance
architecture
code_health
coverage
mutation
metamorphic
security_policy
```

A Playwright test, unit test, API replay, static architecture check or human decision may satisfy different obligations.

---

## 8. OracleSeal

Purpose: prevent implementation-biased “healing”.

```rust
pub struct OracleSeal {
    pub schema_v: u32,
    pub id: OracleSealId,
    pub change: ChangeRef,
    pub requirement_hashes: Vec<ContentHash>,
    pub scenario_hashes: Vec<ContentHash>,
    pub obligation_hashes: Vec<ContentHash>,
    pub quality_policy_hash: ContentHash,
    pub digest: ContentHash,
}
```

Automatic repair is allowed to change:

```text
locator mechanics
setup path
fixture plumbing
deterministic wait
runner syntax
non-semantic wrappers
```

It is forbidden to change without a new seal:

```text
expected business result
permission expectation
boundary/invariant
contract response/error
semantic output
whether behavior exists
whether destructive action is allowed
```

A contradiction to the sealed oracle is a regression/spec decision, not a healing opportunity.

---

# 9. Quality Debt Ratchet

The product must distinguish legacy debt from debt introduced by a PR.

```text
existing debt → visible but not newly blamed on PR
new debt      → warn/block by policy
fixed debt    → explicitly credited
returned debt → regression
excepted      → visible with provenance/expiry
```

This pattern directly extends capabilities already present in Weavatrix Architecture Firewall: stable fingerprints, baseline `existing/new/fixed`, warning severity, runtime-cycle budgets, `maxFileLoc`, `maxFunctionLoc` and explicit exceptions.

Domain state:

```rust
pub enum DebtState {
    Existing,
    New,
    Fixed,
    Returned,
    Excepted,
    Warning,
    ApproachingBudget,
}
```

Default philosophy:

- unchanged old debt: does not block migration;
- changed file already over a budget and grows further: new regression;
- fixed violation returns: error;
- new high-confidence debt in changed production code: warn/error;
- uncertainty remains uncertainty.

---

# 10. Weavatrix-derived Quality Gate catalogue

The rule is: **derive first, add new analyzers only when existing Weavatrix evidence cannot support the check truthfully.**

## 10.1 Dead-code regression

Source: `find_dead_code` + graph/base/head diff.

### WVQ-DEAD-001 — new dead production symbol

New/changed production symbol becomes a new high-confidence dead-code candidate.

Default: `WARN`; private deterministic candidates may become `ERROR` by policy.

### WVQ-DEAD-002 — newly orphaned existing symbol

PR removes the last credible inbound production edge to an existing live symbol.

Default: `WARN`.

### WVQ-DEAD-003 — dead public surface

New exported/public surface lacks internal/external/config contract evidence.

Default: `WARN`, never auto-delete.

### WVQ-DEAD-004 — returned dead debt

Previously fixed dead-code fingerprint reappears.

Default: `ERROR`.

### WVQ-DEAD-005 — dead test fixture/helper

New test helper becomes unreferenced.

Default: `INFO/WARN`.

Never erase Weavatrix labels that say dynamic/config/external use is uncertain.

---

## 10.2 Duplicate/clone regression

Source: `find_duplicates`.

### WVQ-CLONE-001 — new clone family

Changed production code creates a new Type-1/2/3 clone family.

### WVQ-CLONE-002 — clone-family growth

Changed code joins or materially expands an existing clone family.

### WVQ-CLONE-003 — cloned sibling risk

A behavior-affecting clone changed in one location but not the sibling clone. Increase test-selection weight for the sibling.

### WVQ-CLONE-004 — duplicate embedded contract

When string comparison evidence is enabled, detect copied SQL/templates/scripts.

### WVQ-CLONE-005 — fixed clone returned

A clone debt fingerprint previously removed returns.

Defaults: Type-1/2 `WARN`, Type-3 `INFO`, boilerplate ignored unless project policy opts in.

---

## 10.3 Architecture regression

Source: `verify_architecture`, `prepare_change`, `graph_diff`.

Weavatrix Architecture Firewall already supports:

```text
direct/transitive forbid
require
allow_only
unresolved-import policy
runtime/type coupling filters
relation kinds
path/group selectors
severity
runtime cycle budget
max file LOC
max function LOC
ratchet fingerprints
exceptions
```

WVQ adds base/head interpretation.

### WVQ-ARCH-001 — new rule violation
New blocking architecture fingerprint. Default `ERROR`.

### WVQ-ARCH-002 — new architecture warning
New warn-severity violation. Default `WARN`.

### WVQ-ARCH-003 — new runtime cycle
Runtime cycle count grows or new cycle intersects changed code. Default `ERROR`.

### WVQ-ARCH-004 — unresolved local import
Changed production file introduces a new unresolved local import. Default `ERROR`.

### WVQ-ARCH-005 — layer bypass
New direct dependency bypasses the established layer. Default `WARN` unless explicit contract blocks it.

### WVQ-ARCH-006 — dependency-direction drift
PR creates reverse coupling against established component direction. Default `WARN`.

### WVQ-ARCH-007 — unmapped architecture target
Governed component begins depending on an unmapped path. Default `WARN`.

### WVQ-ARCH-008 — new exception request/use
Never silently accepted. Result `NEEDS_REVIEW`.

### WVQ-ARCH-009 — exception expiry near
Warn when expiry is within policy window.

### WVQ-ARCH-010 — fixed architecture debt returns
Default `ERROR`.

---

## 10.4 Size/growth regression

Source: Architecture Firewall LOC budgets + base/head diff.

### WVQ-SIZE-001 — file crosses limit
Changed file crosses `maxFileLoc`.

### WVQ-SIZE-002 — oversized file grows further
File was over limit in base and PR adds LOC. This can warn/block growth without forcing immediate full refactor.

### WVQ-SIZE-003 — file approaches limit
Warn at policy ratios such as 80%/90%.

### WVQ-SIZE-004 — function crosses limit
Changed function exceeds `maxFunctionLoc`.

### WVQ-SIZE-005 — oversized function grows
Existing oversized function becomes larger.

### WVQ-SIZE-006 — disproportionate growth
Diff growth is unusually high relative to the feature surface/history. Advisory only until calibrated.

---

## 10.5 Graph-topology drift

Source: `god_nodes`, graph stats, dependents, communities/module map, graph diff.

### WVQ-GRAPH-001 — god-node growth
Changed node enters god-node candidate band or materially increases connectivity.

### WVQ-GRAPH-002 — fan-out growth
Changed file/module adds unusually many outbound dependencies.

### WVQ-GRAPH-003 — fan-in growth
Changed node becomes a much larger hub, increasing future blast radius.

### WVQ-GRAPH-004 — blast-radius inflation
Small diff produces a large increase in reverse dependency radius.

### WVQ-GRAPH-005 — community-boundary leak
New dependency crosses deterministic module/community boundary not present in baseline.

### WVQ-GRAPH-006 — accidental centralization
New utility/service becomes central to otherwise unrelated communities.

### WVQ-GRAPH-007 — structural orphan
Changed module loses expected connections and becomes isolated.

Topology findings must include base/head numbers; no opaque score.

---

## 10.6 API/transport drift

Source: `list_endpoints`, `trace_endpoint`, `trace_api_contract`, graph diff.

Weavatrix supports HTTP, GraphQL, gRPC and multiple event transports.

### WVQ-API-001 — endpoint removed without spec removal
Externally visible operation disappears without matching OpenSpec REMOVED requirement. Default `ERROR`.

### WVQ-API-002 — endpoint added without spec
New behavior-facing endpoint with no capability/change or explicit no-spec rationale. Default `WARN/ERROR`.

### WVQ-API-003 — producer/consumer drift
Event producer/consumer relation changes on one side without companion proof. Default `WARN`.

### WVQ-API-004 — cross-repo contract impact
Registered dependent repo is affected but receives no companion verification. Default `WARN`.

### WVQ-API-005 — handler graph drift
Route remains but handler graph changes materially and existing proof does not cover the new path.

### WVQ-API-006 — impacted contract unproven
High-risk impacted API has no runtime Proof.

Schema-level backwards compatibility requires a dedicated OpenAPI/GraphQL/Proto adapter before WVQ claims exact schema breakage.

---

## 10.7 Coverage/test regression

Source: `coverage_map`, `select_tests`, normalized runtime coverage.

### WVQ-COV-001 — changed code has no measured coverage
High-risk changed region has no dynamic coverage.

### WVQ-COV-002 — impacted obligation has no executable proof path
Required OpenSpec obligation maps to impacted code but no existing test/session/program can prove it. Default `ERROR` for required obligations.

### WVQ-COV-003 — impacted coverage regression
Base impacted coverage is higher than head beyond configured threshold.

### WVQ-COV-004 — new executable region uncovered
New production regions are unmeasured.

### WVQ-COV-005 — proof-bearing test removed
A test/session proving an active obligation disappears without replacement or spec removal. Default `ERROR`.

### WVQ-COV-006 — static/dynamic selection disagreement
Weavatrix static selection and measured historical coverage disagree strongly. High-risk policy runs the union and learns from result.

### WVQ-COV-007 — hot path with weak coverage
Changed `hot_path_review` candidate lacks strong measured coverage.

---

## 10.8 History/co-change risk

Source: `git_history`, `cross_repo_git`, temporal memory.

### WVQ-HIST-001 — co-change partner omitted
A and B historically co-change for this feature; A changed, B did not. Advisory `WARN`, not automatic correctness failure.

### WVQ-HIST-002 — repeated regression area
Changed graph region has repeated historical regressions. Raises risk/selection weight.

### WVQ-HIST-003 — churn hotspot
High-churn + high-connectivity code changes with weak proof.

### WVQ-HIST-004 — revert-prone region
Similar changes were historically reverted. Raises risk and human visibility.

### WVQ-HIST-005 — cross-repo co-change missing
Historically coupled repo/contract lacks companion change/verification.

---

## 10.9 Runtime/capability health

Source: `run_audit`, `build_graph`.

Possible axes:

```text
new runtime health finding
new dependency health finding
lost analyzer capability/evidence
new unresolved evidence
runner/test-target disappearance
workspace/build configuration drift
```

WVQ preserves evidence axes instead of flattening `run_audit` to one score.

---

## 10.10 Compound risk findings

Some combinations are much more useful than individual checks.

Example:

```text
changed region
AND high graph connectivity
AND high historical churn
AND low measured coverage
AND previous regressions
```

→ `HIGH_RISK_HOTSPOT`.

Represent it transparently:

```yaml
risk_evidence:
  - kind: graph_connectivity
    level: high
  - kind: churn
    level: high
  - kind: measured_coverage
    level: low
  - kind: prior_regressions
    count: 4
```

No magic `risk=87%`.

---

## 10.11 Fixed-debt credit

WVQ must explicitly report improvements:

```text
architecture violations fixed
clone family removed
dead-code candidates removed
runtime cycle removed
file moved below LOC budget
hot-path coverage improved
```

The system should reward cleanup as clearly as it reports new debt.

---

# 11. Additional derived checks

These do not require a new code parser.

### Spec-to-code drift

```text
behavior-facing CodeDelta
AND no SpecDelta
```

→ warn unless explicit refactor/no-behavior-change classification exists.

### Spec-without-implementation

```text
SpecDelta
AND no credible CodeDelta/BehaviorDelta
```

→ `UNPROVEN/ERROR`.

### Production change without relevant test evidence

Risk signal, not absolute failure.

### Removed requirement leaves residue

For an OpenSpec REMOVED requirement, check:

```text
old route remains
handler becomes dead
old test remains
feature flag/config residue remains
legacy duplicate implementation remains
```

### New dependency/config surface

Use manifests/lockfiles and graph relation changes to expose new dependencies and runtime/infrastructure coupling. License/vulnerability scanning remains a separate authoritative adapter.

### Infrastructure/config drift

Where graph evidence contains relations such as `deploys`, `exposes`, `mounts`, `configures`, `reads`, `writes`, a PR can warn about new topology/config coupling.

### Test architecture health

Apply separate policies to tests:

```text
duplicate setup growth
giant test file growth
dead fixture
shared mutable fixture fan-in
ordering-dependent behavior discovered dynamically
```

Production and test debt policies are independently configurable.

---

# 12. Repository quality policy

File:

```text
.weavatrix-quality/config.yaml
```

Example:

```yaml
quality_policy_v: 1

ratchet:
  mode: no_new_debt
  returned_debt: error

architecture:
  use_weavatrix_contract: true

size:
  warn_ratio: 0.80
  strong_warn_ratio: 0.90
  block_growth_when_already_over: false

dead_code:
  new_private_production: warn
  returned: error

duplicates:
  type1: warn
  type2: warn
  type3: info
  boilerplate: ignore

graph_drift:
  god_node_growth: warn
  blast_radius_growth_ratio: 2.0
  community_cross_edge: info

coverage:
  required_for_high_risk: true
  max_impacted_line_regression_percent: 2.0

api:
  removed_without_spec: error
  impacted_without_proof: warn

ai:
  max_tokens_per_change: 20000
  max_runtime_tokens: 6000
  max_browser_escape_calls: 2
  max_vision_calls: 1
  crop_only: true
  on_budget_exhausted: human_required

artifacts:
  success:
    screenshot: false
    trace: false
    har: summarized
  failure:
    screenshot: true
    trace: true
    har: full

gates:
  pull_request:
    block_on: [error]
    require_no_unproven_high_risk: true
```

---

# 13. Risk engine

Risk is evidence-based:

```rust
pub enum RiskEvidenceKind {
    RequirementCriticality,
    CodeBlastRadius,
    ArchitectureBoundary,
    PublicApiChange,
    HistoricalRegression,
    ChurnHotspot,
    LowCoverage,
    NewBehaviorState,
    PermissionChange,
    DataMigration,
    CrossRepoImpact,
    MutationSurvivor,
}
```

Levels:

```text
low
medium
high
critical
```

Risk controls execution breadth, browser matrix, mutation requirement, differential replay, human review requirement and AI budget.

---

# 14. TestProgram IR

Canonical tests are typed programs, not Playwright source.

```rust
pub struct TestProgram {
    pub schema_v: u32,
    pub id: TestProgramId,
    pub source: ProgramSource,
    pub obligations: Vec<ObligationRef>,
    pub preconditions: Vec<Precondition>,
    pub steps: Vec<TestStep>,
    pub evidence_policy: EvidencePolicy,
    pub deterministic_seed: Option<u64>,
}
```

Actions:

```rust
pub enum TestAction {
    Navigate { route: RouteRef },
    Activate { target: Target },
    Fill { target: Target, value: DataRef },
    Select { target: Target, value: DataRef },
    Press { target: Option<Target>, key: KeyRef },
    Wait { condition: WaitCondition },
    SetFeatureFlag { key: String, value: Scalar },
    InjectFault { fault: FaultRef },
    ApiCall { operation: ApiOperationRef, input: DataRef },
    Assert { obligation: ObligationRef },
}
```

UI target:

```rust
pub struct Target {
    pub role: Option<String>,
    pub accessible_name: Option<String>,
    pub label: Option<String>,
    pub test_id: Option<String>,
    pub component_hint: Option<String>,
    pub scope: Option<TargetScope>,
    pub fallback_css: Option<String>,
}
```

Preferred identity:

```text
explicit stable test-id when project policy says stable
→ role + accessible name
→ label
→ ARIA semantics
→ scoped text
→ component fingerprint
→ structural fallback
→ CSS
```

XPath is not a default identity.

---

# 15. Runner architecture

```rust
pub trait Executor {
    fn capabilities(&self) -> ExecutorCapabilities;
    fn prepare(&mut self, request: PrepareRequest) -> Result<PreparedRun>;
    fn execute(&mut self, run: &PreparedRun) -> Result<ExecutionResult>;
}
```

v1:

### TypeScript / JavaScript

```text
Vitest
Jest
Node test where explicitly configured
existing package scripts mapped to registered executor IDs
```

### Bun

Use Bun’s current deterministic interfaces:

```text
bun test
JUnit reporter
LCOV coverage
```

### Go

```text
go test -json
go test -coverprofile
optional go test -race
selected Go fuzz targets
```

### Browser

```text
existing Playwright Test suites
WVQ TestProgram execution through thin TS bridge
trace/network/storage/screenshot only according to EvidencePolicy
```

WVQ does not invent replacement runners.

---

# 16. Browser token economy

Normal browser path:

```text
TestProgram
→ Playwright adapter
→ semantic action
→ structured observation
→ deterministic assertion
```

LLM calls: zero.

Missing target:

```text
stable test-id alias
→ role/name recovery
→ label/ARIA
→ component fingerprint
→ historical aliases
→ only then AgentDecisionRequest
```

Exploration:

```text
coverage-guided deterministic explorer
→ state novelty
→ heuristics
→ tarpit detector
→ bounded agent escape call
```

Screenshots do not enter an LLM by default.

Visual pipeline:

```text
base/head screenshot
→ native deterministic diff
→ identify changed region
→ crop
→ only unresolved crop may reach vision model
```

---

# 17. Recorder and BehaviorGraph

Instrument manual QA.

Capture:

```text
navigate
activate
fill/select/keyboard
route transition
DOM/a11y digest
network metadata
console error/warn
storage mutation
feature flag
viewport
runtime coverage delta
screenshot only by policy
```

A manual session becomes `BehaviorTrace`.

After session, compute:

```text
existing obligation coverage
new obligation coverage
new behavior states
new API operations
new code coverage
redundant steps
candidate minimal replay path
```

“Promote useful path” creates a versioned replay/TestProgram linked to the relevant obligation.

**Rule:** valuable manual testing should become regression knowledge instead of disappearing after one run.

---

# 18. Differential base/head replay

For impacted behavior, run identical:

```text
TestProgram
fixture
seed
clock
feature flags
auth state
viewport
network replay policy
```

on base and head.

Compare in this order:

```text
route
a11y structure
DOM semantics
visible semantic text
normalized component state
network operation set/shape
console
storage
geometry
pixel diff
vision only for unresolved cropped regions
```

This produces `BehaviorDelta`.

---

# 19. Impact-based minimal regression

Candidate evidence:

```text
Weavatrix static graph distance
dynamic coverage overlap
OpenSpec obligation overlap
BehaviorGraph state overlap
historical failure overlap
clone sibling risk
hot-path risk
API-contract overlap
mutation-survivor overlap
risk level
execution cost
flake penalty
```

Pipeline:

```text
all tests/sessions/programs
→ Weavatrix candidate set
→ dynamic coverage intersection
→ obligation/behavior candidate set
→ mandatory high-risk constraints
→ weighted set cover
→ minimal execution set
```

High/critical-risk policy can run a larger union.

Every selected test carries an explanation chain, e.g.:

```text
REQ-17
→ scenario S2
→ Sankey component
→ buildSankeyData
→ changed in head
→ T14 has measured coverage + prior Proof
```

---

# 20. QualityHeuristicsRegistry

Instead of “think of edge cases”, ship deterministic knowledge.

### Numeric

```text
min
min+1
default-1
default
default+1
max-1
max
out-of-range
```

### Collections

```text
0
1
limit-1
limit
limit+1
large
duplicates
missing
null
unstable ordering
```

### Async

```text
loading
success
empty
error
retry
slow
refresh
out-of-order
double submit
cancel
```

### Permission

```text
admin
operator
viewer
tenant mismatch
expired auth
missing capability
```

### UI

```text
empty
long label
missing label
resize
keyboard-only
reload
back/forward
modal interruption
concurrent refresh
```

### API

```text
missing field
extra field
wrong type
boundary
timeout
retry
idempotency
duplicate request
pagination
stream interruption
```

### Go-specific

```text
nil/error
context cancellation
deadline
race-sensitive path
table boundaries
fuzz seed
concurrent access
```

An agent may map a novel requirement to heuristic families once. Generation and execution stay deterministic.

---

# 21. Mutation layer

Only changed/affected code by default.

TS/JS operators:

```text
> ↔ >=
< ↔ <=
=== ↔ !==
true ↔ false
&& ↔ ||
+1 ↔ -1
remove branch
remove sort
wrong permission
omit callback
omit error propagation
wrong collection boundary
```

Go:

```text
err != nil ↔ err == nil
boundary flip
return nil/zero
skip branch
ignore context
invert boolean
```

Proof can say:

```text
execution coverage: yes
mutation killed: 8
mutation survived: 1
verdict: PARTIAL
```

A survived relevant mutant is explicit proof weakness, not hidden behind line coverage.

---

# 22. Metamorphic layer

Versioned `MetamorphicRelation`.

Analytics examples:

```text
permute(input) → aggregate unchanged
append(zero record) → SUM unchanged
scale(values, 2) → SUM doubles
split/recombine groups → total conserved
viewport change → data semantics unchanged
```

Agent may propose a relation once; QA approves/seals it; subsequent executions require zero AI.

---

# 23. Flake Lab

Fingerprint:

```rust
pub struct FailureFingerprint {
    pub test_program: TestProgramId,
    pub obligation: Option<ObligationRef>,
    pub revision: RevisionRef,
    pub seed: Option<u64>,
    pub executor: ExecutorId,
    pub browser: Option<BrowserId>,
    pub state_digest: Option<ContentHash>,
    pub stack_digest: Option<ContentHash>,
    pub console_digest: Option<ContentHash>,
    pub network_digest: Option<ContentHash>,
    pub timing_bucket: TimingBucket,
}
```

Deterministic triage order:

```text
known fingerprint
stable same-state product regression
ordering dependence
timeout/timing distribution
network instability
environment mismatch
selector drift
data/seed dependence
test-order dependence
unknown
```

Only `unknown` should request bounded AI context.

---

# 24. Safe healing

Automatic:

```text
locator alias
deterministic wait
fixture plumbing
runner syntax
non-semantic wrapper path
```

Requires:

```text
same OracleSeal
same obligations
same semantic assertions
```

Forbidden:

```text
changing expected behavior
deleting assertion to go green
weakening permissions
accepting new baseline automatically
```

---

# 25. Cheap explorer

Default is model-less.

Score:

```text
uncovered obligation
+ state novelty
+ graph-risk proximity
+ boundary heuristic
+ historical bug similarity
- already covered action
- expensive setup
```

Tarpit:

```text
N actions
AND no new behavior state
AND no new obligation coverage
AND no new code coverage
```

Only then request a bounded semantic escape action.

Agent packet:

```text
goal
uncovered obligation
current state digest
top semantic controls
last five actions
failed deterministic candidates
```

No giant DOM/screenshot context unless budget explicitly allows it.

---

# 26. AI Cost Firewall

```rust
pub struct AiBudget {
    pub planning_tokens: u64,
    pub runtime_tokens: u64,
    pub browser_escape_calls: u32,
    pub vision_calls: u32,
    pub max_cost_micros: Option<u64>,
}
```

Invariant:

```text
ordinary green path runtime tokens = 0
```

Budget exhaustion:

```text
HUMAN_REQUIRED
reason = AI_BUDGET_EXHAUSTED
```

Never silently escalate models.

Telemetry:

```text
tokens/change
tokens/generated obligation
tokens/ambiguous failure
QA tokens / development tokens
browser escape calls
vision calls
cost/model
cache hit ratio
```

Avoid nested hidden AI:

```text
external Claude/Codex
→ WVQ MCP
→ deterministic WVQ
→ optional DecisionRequest back to same external agent
```

not:

```text
Claude → WVQ → hidden GPT → hidden browser agent → ...
```

---

# 27. Evidence Ledger and Proof

```rust
pub struct EvidenceArtifact {
    pub id: ArtifactId,
    pub kind: EvidenceKind,
    pub revision: RevisionRef,
    pub content_hash: ContentHash,
    pub producer: ProducerRef,
    pub created_at: Timestamp,
    pub metadata: BTreeMap<String, Scalar>,
}
```

`Proof`:

```rust
pub struct Proof {
    pub schema_v: u32,
    pub id: ProofId,
    pub requirement: RequirementRef,
    pub scenario: ScenarioRef,
    pub obligation: ObligationRef,
    pub oracle_seal: OracleSealId,
    pub revision: RevisionRef,
    pub program: Option<TestProgramId>,
    pub run: Option<RunId>,
    pub observations: Vec<ObservationRef>,
    pub artifacts: Vec<ArtifactId>,
    pub mutation: Option<MutationSummary>,
    pub verdict: ProofVerdict,
}
```

Verdicts:

```text
PROVEN
CONTRADICTED
PARTIAL
UNPROVEN
HUMAN_REQUIRED
```

Never collapse to one percentage.

---

# 28. Storage

SQLite + CAS.

Tables:

```text
repositories
revisions
changes
requirements
scenarios
obligations
oracle_seals
quality_policies
test_programs
program_obligations
executors
runs
run_items
observations
behavior_states
behavior_edges
artifacts
proofs
proof_artifacts
quality_findings
debt_fingerprints
debt_baselines
coverage_regions
test_coverage_bitmaps
behavior_coverage_bitmaps
failure_fingerprints
failure_occurrences
mutation_cases
mutation_results
manual_sessions
ai_usage
human_decisions
```

CAS:

```text
.weavatrix-quality/
  quality.db
  objects/
    ab/
      abcdef...
```

Use compact bitmaps for dynamic coverage and fast set intersections where beneficial.

---

# 29. MCP surface

Implement with `mcport`.

Default coding-agent profile:

```text
quality_context
quality_plan
quality_run
quality_status
quality_verify
quality_explain
quality_evidence
```

### `quality_context`

Input:

```json
{
  "change": "current",
  "purpose": "implementation",
  "token_budget": 4000
}
```

Returns bounded `QualityContextPacket`.

### `quality_plan`

Returns requirements, obligations, risk evidence, existing proofs, gaps and deterministic checks. No execution.

### `quality_run`

```json
{
  "change": "current",
  "scope": "impacted",
  "evidence_policy": "standard"
}
```

No arbitrary shell.

### `quality_status`

Compact progress + handles.

### `quality_verify`

Returns multi-axis quality verdict.

### `quality_explain`

Explains one finding/failure/selection/proof with exact provenance.

### `quality_evidence`

Returns bounded metadata/small textual artifact; large binary data remains handle-based.

Advanced QA profile adds:

```text
quality_select
quality_explore
quality_replay
quality_promote
quality_mutate
quality_baseline
quality_debt
```

---

# 30. CLI / HTTP

CLI:

```text
wvq init
wvq spec validate
wvq spec seal
wvq analyze
wvq debt
wvq select
wvq run
wvq verify
wvq explain <id>
wvq record
wvq replay
wvq baseline
wvq doctor
```

HTTP serves Quality Studio. MCP is agent-only.

All surfaces share one command bus:

```text
CLI ─┐
HTTP ├→ wvq-command-bus → domain services
MCP ─┘
```

---

# 31. Quality Studio

Main objects:

```text
Change
Requirement
Proof
Finding
HumanDecision
```

Main UI is exception-first:

```text
┌──────────────────────────────────────────────────────────┐
│ Weavatrix Quality                    AI: 1.8k / 20k      │
├──────────────────────────────────────────────────────────┤
│ change: sankey-others                                    │
│ Requirements      8                                      │
│ Proven            7                                      │
│ New quality debt  0 errors · 2 warnings                  │
│ Unexpected delta  1                                      │
│                                                          │
│ NEEDS HUMAN                                              │
│ R18-S3: refresh while Others dialog is open              │
│ base: dialog remains open                                │
│ head: dialog closes                                      │
│ OpenSpec: unspecified                                    │
│                                                          │
│ [Expected change] [Bug] [Update spec]                    │
└──────────────────────────────────────────────────────────┘
```

Do not show hundreds of successful tests as the primary UX.

---

# 32. CI output

```text
SPEC
CODE HEALTH
ARCHITECTURE
COVERAGE
BEHAVIOR
MUTATION
FLAKE
AI BUDGET
PROOF
```

Example:

```text
SPEC
  8 requirements
  15 scenarios
  14 proven
  1 human-required

CODE HEALTH
  new dead code       0
  new clone families  1 WARN
  fixed debt          2

ARCHITECTURE
  new violations      0
  runtime cycles      unchanged
  file size           1 approaching budget WARN

COVERAGE
  impacted regions    29
  measured            29
  regression          0

BEHAVIOR
  expected delta      4
  unexpected delta    1

MUTATION
  killed              18
  survived            1 WARN

FLAKE
  unresolved          0

AI
  runtime tokens      0
  planning tokens     1820

VERDICT
  NEEDS_HUMAN
```

---

# 33. v1 definition of done

A real TS/JS/Bun/Go repository can:

1. parse OpenSpec + `quality.yaml`;
2. seal obligations;
3. compare base/head Weavatrix evidence;
4. run Quality Debt Ratchet;
5. select a minimal impacted test subset;
6. execute Vitest/Jest/Bun/Go/Playwright existing tests;
7. normalize JUnit/JSON/LCOV/Go coverage evidence;
8. create revision-bound Proofs;
9. expose `quality_verify` over CLI/MCP;
10. separate `new/existing/fixed/returned` debt;
11. consume zero LLM runtime tokens for a normal green PR.

Recorder/explorer/mutation are staged after this usable loop.

---

# 34. Suggested repo structure

```text
weavatrix-quality/
├── Cargo.toml
├── rust-toolchain.toml
├── README.md
├── LICENSE
├── crates/
│   ├── wvq-domain/
│   │   └── src/{lib,ids,spec,program,evidence,proof,finding,policy}.rs
│   ├── wvq-spec/
│   │   └── src/{lib,openspec,quality_yaml,obligations,seal}.rs
│   ├── wvq-intelligence/
│   │   └── src/
│   │       ├── {lib,weavatrix,delta,debt,risk,heuristics,selection}.rs
│   │       └── checks/{mod,dead_code,duplicates,architecture,size,topology,api,coverage,history}.rs
│   ├── wvq-runtime/
│   │   └── src/{lib,executor,process,normalize,junit,lcov,gojson,browser_protocol}.rs
│   ├── wvq-proof/
│   │   └── src/{lib,assemble,behavior,verdict,flake,mutation,differential}.rs
│   ├── wvq-store/
│   │   └── src/{lib,migrations,sqlite,cas,repository}.rs
│   └── wvq-command-bus/
│       └── src/{lib,commands,replies,service}.rs
├── apps/
│   ├── wvq-cli/
│   ├── wvq-mcp/
│   └── qualityd/
├── js/playwright-runner/
│   └── src/{main,protocol,execute,observe,record}.ts
├── studio/
├── schemas/
│   ├── quality-contract-v1.schema.json
│   ├── test-program-v1.schema.json
│   └── proof-v1.schema.json
├── fixtures/{openspec,ts-vitest,bun,go,browser}/
└── docs/{architecture,policy,mcp,proof-model,research}.md
```

---

# 35. Milestones

| Milestone | Deliverable |
|---|---|
| M0 | shadow benchmark harness |
| M1 | domain + OpenSpec + OracleSeal |
| M2 | Weavatrix integration + Quality Debt Ratchet |
| M3 | runner normalization + minimal selection |
| M4 | Proof/Evidence + CLI/MCP |
| M5 | browser TestProgram bridge |
| M6 | recorder + BehaviorGraph + base/head diff |
| M7 | deterministic triage + safe healing |
| M8 | mutation + metamorphic |
| M9 | cheap explorer + AI Cost Firewall |
| M10 | Quality Studio |
| M11 | Figma/advanced visual/cross-repo integrations |

---

# Part II — Detailed implementation plan

## 36. Task 1 — workspace and domain contracts

**Files**
- Create `Cargo.toml`
- Create `rust-toolchain.toml`
- Create `crates/wvq-domain/Cargo.toml`
- Create `crates/wvq-domain/src/lib.rs`
- Create `crates/wvq-domain/src/ids.rs`
- Create `crates/wvq-domain/src/finding.rs`
- Test `crates/wvq-domain/tests/roundtrip.rs`

**Produces**
`ChangeId`, `RequirementId`, `ScenarioId`, `ObligationId`, `ProgramId`, `RunId`, `ProofId`, `ArtifactId`, `CheckId`, `ContentHash`.

- [ ] Write failing typed-ID roundtrip test:

```rust
#[test]
fn requirement_id_round_trips() {
    let id = wvq_domain::RequirementId::new("sankey.visual-limit").unwrap();
    let json = serde_json::to_string(&id).unwrap();
    let back: wvq_domain::RequirementId = serde_json::from_str(&json).unwrap();
    assert_eq!(id, back);
}
```

- [ ] Run `cargo test -p wvq-domain`; verify compile failure.
- [ ] Implement non-empty typed string IDs with `Display`, `FromStr`, Serde.
- [ ] Add `Severity`, `FindingState`, `SubjectRef`, `QualityFinding`.
- [ ] Run tests; expect PASS.
- [ ] Commit:

```bash
git add Cargo.toml rust-toolchain.toml crates/wvq-domain
git commit -m "feat(domain): add stable quality contracts"
```

---

## 37. Task 2 — OpenSpec compatibility reader

**Files**
- `crates/wvq-spec/Cargo.toml`
- `crates/wvq-spec/src/{lib,openspec}.rs`
- `fixtures/openspec/...`
- `crates/wvq-spec/tests/openspec_reader.rs`

**Produces**

```rust
pub fn read_change(root: &Path, change: &str) -> Result<OpenSpecChange>;
```

- [ ] Add fixture with ADDED and MODIFIED requirements using exact `### Requirement` / `#### Scenario` headers.
- [ ] Write failing parser test asserting operation, requirement text, scenario and GIVEN/WHEN/THEN.
- [ ] Implement strict parser preserving source file/line provenance.
- [ ] Add REMOVED/RENAMED fixtures.
- [ ] Reject malformed nesting.
- [ ] Run `cargo test -p wvq-spec`.
- [ ] Commit `feat(spec): read OpenSpec change deltas`.

---

## 38. Task 3 — quality contract and OracleSeal

**Files**
- `wvq-spec/src/{quality_yaml,obligations,seal}.rs`
- `schemas/quality-contract-v1.schema.json`
- tests `quality_contract.rs`, `oracle_seal.rs`

**Produces**

```rust
load_quality_contract(...)
compile_obligations(...)
seal(...)
```

- [ ] Test duplicate obligation IDs fail.
- [ ] Test unknown scenario references fail.
- [ ] Test unknown evidence kinds fail.
- [ ] Implement strict YAML + semantic validation.
- [ ] Implement canonical serialization before hashing.
- [ ] Test implementation-only metadata changes do not alter seal.
- [ ] Test expected-invariant change does alter seal.
- [ ] Commit `feat(spec): compile and seal quality obligations`.

---

## 39. Task 4 — embed `weavatrix-rust`

**Files**
- `crates/wvq-intelligence/Cargo.toml`
- `src/{lib,weavatrix}.rs`
- `tests/revision_analysis.rs`

**Interface**

```rust
pub trait CodeEvidenceProvider {
    fn analyze(&self, repo: &Path) -> Result<RepoEvidence>;
    fn operation(&self, repo: &Path, name: &str, args: Value) -> Result<Value>;
}
```

- [ ] Add fixture-repository failing analysis test.
- [ ] Depend on `weavatrix-rust`.
- [ ] Implement `WeavatrixProvider`.
- [ ] Assert every result has revision identity.
- [ ] Explicitly document “no second parser/code graph”.
- [ ] Commit `feat(intelligence): embed weavatrix engine`.

---

## 40. Task 5 — generic Quality Debt Ratchet

**Files**
- `wvq-intelligence/src/debt.rs`
- `wvq-domain/src/finding.rs`
- `tests/debt_ratchet.rs`

**Interface**

```rust
classify_debt(base, head, baseline) -> DebtDelta
```

- [ ] Test existing/new/fixed/returned.
- [ ] Test stable fingerprint ordering independence.
- [ ] Test fixed debt reintroduced → returned.
- [ ] Implement fingerprint canonicalization.
- [ ] Commit `feat(quality): add no-new-debt ratchet`.

---

## 41. Task 6 — Architecture + size gates

**Files**
- `wvq-intelligence/src/checks/{mod,architecture,size}.rs`
- `tests/architecture_checks.rs`

- [ ] Fixture: existing oversized file unchanged → existing debt.
- [ ] Fixture: existing oversized file grows → new warning.
- [ ] Fixture: head creates runtime cycle → error.
- [ ] Fixture: warn-severity architecture violation → warning.
- [ ] Map Weavatrix structured architecture result into WVQ IDs.
- [ ] Include original Weavatrix fingerprint/evidence in finding.
- [ ] Commit `feat(checks): gate architecture and size regressions`.

---

## 42. Task 7 — dead-code + clone delta

**Files**
- `checks/{dead_code,duplicates}.rs`
- `tests/health_delta.rs`

- [ ] Fixture where head orphans helper.
- [ ] Fixture where changed code joins clone family.
- [ ] Preserve dynamic/config/external ambiguity labels.
- [ ] Classify returned debt.
- [ ] Never provide auto-delete action.
- [ ] Commit `feat(checks): detect new dead code and clones`.

---

## 43. Task 8 — topology drift

**Files**
- `checks/topology.rs`
- `tests/topology_delta.rs`

- [ ] Test fan-out growth.
- [ ] Test reverse blast-radius growth.
- [ ] Test new community-crossing edge.
- [ ] Test god-node growth.
- [ ] Every finding includes base/head numeric evidence.
- [ ] Commit `feat(checks): report graph topology drift`.

---

## 44. Task 9 — API + history risk

**Files**
- `checks/{api,history}.rs`
- `risk.rs`
- `tests/api_history.rs`

- [ ] Test endpoint removed with no OpenSpec REMOVED delta → error.
- [ ] Test impacted contract without Proof → warning/error by risk.
- [ ] Test historical co-change omission.
- [ ] Implement `RiskEvidence[]`, not opaque percentage.
- [ ] Commit `feat(checks): connect contracts and historical risk`.

---

## 45. Task 10 — runner result normalization

**Files**
- `wvq-runtime/src/{lib,normalize,junit,lcov,gojson}.rs`
- fixtures for Vitest/Bun/Go
- `tests/normalization.rs`

**Output**

```rust
pub struct NormalizedTestRun {
    pub cases: Vec<TestCaseResult>,
    pub coverage: Option<CoverageArtifact>,
    pub raw_artifacts: Vec<ArtifactDescriptor>,
}
```

- [ ] Parse representative JUnit.
- [ ] Parse LCOV to source ranges.
- [ ] Parse `go test -json`.
- [ ] Reject malformed/truncated evidence.
- [ ] Add Bun JUnit/LCOV fixture.
- [ ] Commit `feat(runtime): normalize test evidence`.

---

## 46. Task 11 — bounded executor registry

**Files**
- `wvq-runtime/src/{executor,process}.rs`
- `tests/executor_registry.rs`

- [ ] Unknown executor ID must fail.
- [ ] Registered commands receive bounded typed args.
- [ ] Add deadline, output-size and cancellation limits.
- [ ] No MCP/user field can inject arbitrary executable command.
- [ ] Commit `feat(runtime): add registered bounded executors`.

---

## 47. Task 12 — dynamic coverage ↔ Weavatrix

**Files**
- `checks/coverage.rs`
- `tests/coverage_mapping.rs`

- [ ] Map LCOV/source ranges to graph nodes.
- [ ] Compare impacted base/head coverage.
- [ ] Detect changed high-risk graph node with no measured coverage.
- [ ] Preserve static-reachability vs measured-coverage distinction.
- [ ] Commit `feat(coverage): map runtime evidence to impacted code`.

---

## 48. Task 13 — minimal selection

**Files**
- `selection.rs`
- `tests/selection.rs`

**Interface**

```rust
select_minimal_plan(input: SelectionInput) -> SelectionPlan
```

- [ ] Synthetic matrix where multiple tests cover same obligation.
- [ ] Mandatory high-risk obligation can never be omitted.
- [ ] Prefer cheaper equivalent candidate.
- [ ] Include explanation chain in each selection.
- [ ] Implement deterministic greedy weighted set cover first.
- [ ] Benchmark before considering solver complexity.
- [ ] Commit `feat(selection): choose minimal impacted regression`.

---

## 49. Task 14 — SQLite + CAS

**Files**
- `wvq-store/src/{lib,migrations,sqlite,cas,repository}.rs`
- `tests/store.rs`

- [ ] Test content hash deduplication.
- [ ] Test transaction rollback no dangling artifact ref.
- [ ] Test Proof immutability.
- [ ] Add schema-version migration table.
- [ ] Store large blobs only in CAS.
- [ ] Commit `feat(store): add evidence ledger`.

---

## 50. Task 15 — Proof engine

**Files**
- `wvq-proof/src/{lib,assemble,verdict}.rs`
- `tests/verdict.rs`

- [ ] Passing required execution → `PROVEN`.
- [ ] Missing required runtime evidence → `UNPROVEN`.
- [ ] Sealed expectation contradicted → `CONTRADICTED`.
- [ ] Spec ambiguity → `HUMAN_REQUIRED`.
- [ ] Quality debt remains separate evidence axis.
- [ ] Commit `feat(proof): assemble revision-bound verdicts`.

---

## 51. Task 16 — command bus, CLI and MCP

### Command bus

**Files**
- `wvq-command-bus/src/{lib,commands,replies,service}.rs`

Commands:

```text
Context
Plan
Run
Status
Verify
Explain
Evidence
```

- [ ] Fake-provider service tests.
- [ ] Bounded replies.
- [ ] No transport-specific domain logic.

### CLI

- [ ] `wvq spec validate`
- [ ] `wvq spec seal`
- [ ] `wvq analyze`
- [ ] `wvq debt`
- [ ] `wvq select`
- [ ] `wvq run`
- [ ] `wvq verify`
- [ ] `wvq explain`

Block verdict returns non-zero exit code.

### MCP

Implement through `mcport`.

- [ ] Strict schemas.
- [ ] Seven default tools only.
- [ ] Controlled concurrency/deadlines.
- [ ] Large evidence via handle.
- [ ] Benchmark tool-schema token footprint.
- [ ] Commit `feat(product): add CLI and bounded MCP`.

---

## 52. Task 17 — shadow benchmark harness

**Files**
- `crates/wvq-bench/`
- `docs/benchmark-methodology.md`

Metrics:

```text
selected vs full test count
selected vs full wall-clock
human bugs/failures recovered
false-positive findings
false-negative findings
AI tokens
artifact bytes
```

- [ ] Evaluate TS frontend.
- [ ] Evaluate Node/Bun backend.
- [ ] Evaluate Go service.
- [ ] Do not publish “10×” until human-touch-time data exists.
- [ ] Commit `bench: add shadow quality evaluation`.

---

## 53. Task 18 — Browser TestProgram vertical slice

### Rust IR

- [ ] Add `TestProgram`, typed actions and JSON schema.
- [ ] Reject invalid target/action.
- [ ] Add obligation references and deterministic seed.

### Thin TypeScript Playwright bridge

Protocol:

```text
initialize
prepare
execute_step
observe
finish
cancel
```

- [ ] Rust/TS golden protocol fixtures.
- [ ] Unknown message fails closed.
- [ ] No AI logic in TS bridge.
- [ ] Structured observation: route, a11y/DOM digest, network metadata, console, storage, viewport.
- [ ] Screenshot only by EvidencePolicy.
- [ ] Commit `feat(browser): add deterministic TestProgram execution`.

---

## 54. Task 19 — Record/replay + BehaviorGraph

- [ ] Implement normalized `BehaviorState` hash.
- [ ] Persist states/edges.
- [ ] Add semantic manual recorder.
- [ ] Link session to obligations, API operations and coverage.
- [ ] Compute coverage contribution.
- [ ] Generate promotion candidate.
- [ ] Implement replay with same fixture/seed.
- [ ] Commit `feat(behavior): turn manual QA into replayable knowledge`.

---

## 55. Task 20 — Differential behavior

- [ ] Run same TestProgram on base/head.
- [ ] Compare structured observations before pixel comparison.
- [ ] Produce `BehaviorDelta`.
- [ ] Join with SpecDelta/CodeDelta.
- [ ] Classify unexpected delta as explicit finding.
- [ ] Commit `feat(diff): add Delta Triangle verification`.

---

## 56. Task 21 — Flake + safe healing

### Flake

- [ ] Persist failure fingerprints.
- [ ] Cluster repeats.
- [ ] Detect ordering/timing/network/seed/test-order patterns.
- [ ] Create compact `DecisionPacket` only for unknowns.

### Healing

- [ ] Deterministic semantic target recovery.
- [ ] Require same OracleSeal and semantic assertions.
- [ ] Reject expected-result changes.
- [ ] Store repair as versioned program revision.
- [ ] Commit `feat(triage): diagnose and safely heal tests`.

---

## 57. Task 22 — Mutation, metamorphic, cheap explorer

### Mutation
- [ ] Changed-region TS/JS operators.
- [ ] Safe Go operators.
- [ ] Run relevant selected tests only.
- [ ] Attach killed/survived to Proof.

### Metamorphic
- [ ] Define versioned relation.
- [ ] Ship numeric/collection/aggregation built-ins.
- [ ] Agent-proposed relation requires review + seal.
- [ ] Execute model-less.

### Explorer
- [ ] Enumerate semantic controls.
- [ ] Score state novelty + uncovered obligation + risk.
- [ ] Enforce depth/action budget.
- [ ] Detect tarpit.
- [ ] Emit agent decision only after deterministic exhaustion.
- [ ] Commit `feat(advanced): add proof strength and cheap exploration`.

---

## 58. Task 23 — AI Cost Firewall + Quality Studio

### Cost firewall

- [ ] Persist budget per change/run.
- [ ] Reject over-budget AI decisions.
- [ ] `HUMAN_REQUIRED` on exhaustion.
- [ ] Track QA/development token ratio when development data is supplied.
- [ ] Track browser/vision calls.

### Quality Studio

Initial API:

```text
GET /api/v1/changes
GET /api/v1/changes/:id/summary
GET /api/v1/requirements/:id/proofs
GET /api/v1/findings/:id
GET /api/v1/runs/:id
POST /api/v1/human-decisions
GET /api/v1/artifacts/:id
```

Initial screens:

```text
Changes
Needs Human
Requirement / Proof
Quality Debt
Run detail
Policy / AI budget
```

- [ ] Main dashboard hides pass-noise.
- [ ] Human decision is explicit and provenance-bearing.
- [ ] No baseline/spec mutation via implicit “accept all”.
- [ ] Commit `feat(studio): add exception-only QA cockpit`.

---

# 59. CI rollout

## Stage A — observe-only

WVQ runs but never blocks. Compare WVQ findings with QA outcomes for 30–50 historical/current PRs.

## Stage B — block only objective new debt

Good early blockers:

```text
new architecture ERROR
new unresolved local import
removed API without explicit contract removal
unproven required critical/high-risk obligation
OracleSeal contradiction
```

Keep advisory initially:

```text
clone growth
dead-code candidate
god-node growth
co-change mismatch
approaching LOC limit
```

## Stage C — promote calibrated warnings

Promote a category only after repository-specific precision is acceptable.

## Stage D — automatic eligible verdict

Low/medium-risk changes with complete Proof and no blocking debt no longer require manual regression execution.

---

# 60. Priority by leverage

| Capability | Human-QA leverage | AI-cost reduction | Priority |
|---|---:|---:|---:|
| Weavatrix Quality Debt Ratchet | high | very high | P0 |
| impact-based minimal regression | very high | very high | P0 |
| OpenSpec obligations + OracleSeal | very high | high | P0 |
| Proof/Evidence Ledger | very high | high | P0 |
| result/coverage normalization | high | very high | P0 |
| MCP/CLI | high | high | P0 |
| record once / replay forever | extreme | extreme | P1 |
| base/head behavior diff | very high | extreme | P1 |
| deterministic flake triage | very high | high | P1 |
| safe healing | high | high | P1 |
| Quality Studio | high adoption leverage | neutral | P1 |
| targeted mutation | high | high | P2 |
| metamorphic testing | very high for analytics | high | P2 |
| cheap explorer | high | very high vs ReAct | P2 |
| Figma structured oracle | high for UI | moderate | P3 |
| vision-heavy exploration | niche | poor | P4 |

---

# 61. Explicit non-goals

Do not build before evidence proves need:

```text
Rust browser rendering engine
Playwright replacement
generic shell MCP
OpenSpec fork
giant generic multi-agent framework
vision-first browser agent
automatic business-oracle healing
custom JS/Go test runner
Neo4j dependency
one global quality percentage
full Cartesian test matrix
whole-repo mutation on every PR
three browsers for every backend-only change
```

---

# 62. Benchmark acceptance gates

Before calling the system “10×”, measure:

### Human effort
```text
human_QA_minutes/PR
manual_retest_minutes
manual_triage_minutes
human decisions/PR
```

### Quality
```text
regressions caught
escaped regressions
false-positive gate rate
mutation sensitivity
flake rate
```

### Execution
```text
full suite time
selected suite time
browser minutes
CPU
artifact storage
```

### AI
```text
planning tokens
runtime tokens
browser escape calls
vision calls
QA/development token ratio
cost per PR
cost per avoided human exception
```

Required safety constraint:

```text
escaped regressions must not increase as human execution is reduced
```

---

# 63. Primary sources / research references

- Weavatrix Rust: https://github.com/sergii-ziborov/weavatrix-rust
- Weavatrix Architecture Firewall: https://github.com/sergii-ziborov/weavatrix-rust/blob/main/docs/architecture-firewall.md
- OpenSpec: https://github.com/Fission-AI/OpenSpec
- OpenSpec customization/custom schemas: https://github.com/Fission-AI/OpenSpec/blob/main/docs/customization.md
- Playwright CLI: https://github.com/microsoft/playwright-cli
- Playwright test-generation skill: https://github.com/microsoft/playwright-cli/blob/main/skills/playwright-cli/references/test-generation.md
- Web Agents Should Adopt the Plan-Then-Execute Paradigm: https://arxiv.org/abs/2605.14290
- ActionEngine: https://arxiv.org/abs/2602.20502
- KTester / Knowledge Matters, ICSE 2026: https://conf.researchr.org/details/icse-2026/icse-2026-research-track/241/Knowledge-Matters-Injecting-Project-and-Testing-Knowledge-into-LLM-based-Unit-Test-G
- Uber AutoCover, ICSE-SEIP 2026: https://conf.researchr.org/details/icse-2026/icse-2026-software-engineering-in-practice/58/Automated-Software-Test-Generation-at-Industry-Scale-Using-a-Multi-Agent-Architecture
- Panta, ICSE 2026: https://conf.researchr.org/details/icse-2026/icse-2026-research-track/36/LLM-Test-Generation-via-Iterative-Hybrid-Program-Analysis
- Coding-before-testing bias: https://arxiv.org/abs/2607.05139
- SWE-Mutation: https://arxiv.org/abs/2605.22175
- FlakyGuard: https://arxiv.org/abs/2511.14002
- LLM-generated test flakiness: https://arxiv.org/abs/2601.08998
- Bun test/JUnit/coverage: https://bun.com/docs/test
- Go coverage: https://go.dev/doc/build-cover
- Go fuzzing: https://go.dev/doc/security/fuzz/

---

# 64. Final architecture decision

The invariant:

> **AI is the compiler and exception handler, never the normal execution runtime.**

The proof chain:

```text
OpenSpec
   ↓
QualityContract
   ↓
OracleSeal
   ↓
IntentGraph

weavatrix-rust
   ↓
CodeGraph
   ↓
CodeDelta
   ↓
Quality Debt Ratchet
       ├─ dead code delta
       ├─ clone delta
       ├─ architecture drift
       ├─ cycles/unresolved imports
       ├─ file/function size ratchet
       ├─ graph topology/god-node drift
       ├─ API/transport drift
       ├─ coverage drift
       └─ history/hotspot risk

Playwright / JS / Bun / Go
   ↓
TestProgram execution
   ↓
BehaviorGraph
   ↓
BehaviorDelta

SpecDelta + CodeDelta + BehaviorDelta
   ↓
Delta Triangle
   ↓
minimal impacted verification
   ↓
Evidence Ledger
   ↓
Proof
   ↓
mutation / flake / differential
   ↓
QualityVerdict
   ↓
exception-only human QA
```

The moat is the combination:

1. OpenSpec-backed `OracleSeal`.
2. `SpecDelta + CodeDelta + BehaviorDelta`.
3. Weavatrix-powered code-health/architecture debt ratchet.
4. Impact-based minimal regression.
5. Record-once/replay-forever BehaviorGraph.
6. Revision-bound `Proof`.
7. Mutation/metamorphic Proof strength.
8. Deterministic flake/triage/healing.
9. Hard AI Cost Firewall.
10. Human QA focused on ambiguity, novel UX and genuinely new risk.

This is the version of `weavatrix-quality` worth building.

---

# 65. Spec Recovery, Acceptance Criteria Synthesis, and Mandatory QA Verification

WVQ must support brownfield and late-spec workflows where implementation or a pull request already exists before a trustworthy OpenSpec change exists.

The subsystem is:

```text
wvq-spec-recovery
```

It consumes:

```text
PR title/body
linked work item
commit sequence
commit titles/bodies
base/head Weavatrix graph evidence
changed routes/APIs/components/symbols
changed tests
runtime BehaviorDelta
neighboring OpenSpec requirements
historical quality evidence
```

and generates:

```text
candidate OpenSpec delta
candidate acceptance criteria
candidate quality.yaml
ambiguities
questions for QA/product
```

## 65.1 Core safety rule

> **Implementation evidence can propose intent; it cannot establish intent by itself.**

Recovered requirements therefore begin as:

```text
RECOVERED
→ PROPOSED
→ AUTO_CHECKED
→ QA_REVIEW
```

They cannot enter `OracleSeal` until human verification succeeds.

If QA cannot infer intended product behavior from available evidence, the correct state is:

```text
PRODUCT_DECISION_REQUIRED
```

not an AI guess.

## 65.2 Evidence authority hierarchy

### Tier A — declared product intent

```text
existing OpenSpec
approved linked ticket
explicit acceptance criteria
approved product/design decision
PM/product statement
```

### Tier B — reviewed collaboration evidence

```text
PR description
approved review discussion
commit body
release note
approved QA plan
```

### Tier C — implementation evidence

Strong evidence of observed implementation, not normative intent:

```text
CodeDelta
changed route/API
changed component
changed permission path
changed configuration
changed data model
changed tests
BehaviorDelta
```

### Tier D — weak hints

```text
commit title
branch name
file name
symbol name
comment
```

Do not flatten these to a single percentage.

Example:

```yaml
confidence:
  intent_evidence: medium
  implementation_evidence: strong
  behavioral_observation: strong
  oracle_independence: weak
```

## 65.3 Commit titles are naming/grouping hints

A title such as:

```text
fix sankey visual limit
```

does not establish:

```text
actor
precondition
boundary
expected observable result
permissions
API contract
error behavior
refresh/concurrency behavior
whether previous behavior was a bug
```

Commit titles must never directly become normative `SHALL` statements.

## 65.4 Cluster commits into capability changes

Wrong:

```text
commit A → requirement A
commit B → requirement B
commit C → requirement C
```

Correct:

```text
commit sequence
      ↓
base/head graph regions
      ↓
API/component/community grouping
      ↓
capability change clusters
      ↓
candidate requirements/scenarios
```

Clustering priority:

```text
existing OpenSpec capability
linked issue/task
public API/route
Weavatrix module/community
component ownership
dependency neighborhood
commit adjacency
commit-title semantic similarity
```

LLM similarity is the final fallback.

## 65.5 Deterministic ChangeNarrative

Before an LLM call, WVQ produces a compact revision-bound narrative.

Example:

```yaml
change_cluster: sankey-others

declared_intent:
  pr_title: "Add Others node for Sankey visual limit"

commit_hints:
  - add others endpoint
  - group overflow values
  - render Others node

code_delta:
  frontend:
    components: [Sankey]
    changed_symbols:
      - buildSankeyData
      - renderNodes

  backend:
    endpoints_added:
      - GET /api/sankey/others
    changed_symbols:
      - groupOverflow
      - getOthers

tests_delta:
  added:
    - sankey-others.spec.ts

behavior_delta:
  observed:
    - Others node appears above configured visual limit
    - activating Others requests detail endpoint
```

The agent receives this narrative, not the repository.

## 65.6 RecoveryPacket

```rust
pub struct RecoveryPacket {
    pub base_revision: RevisionRef,
    pub head_revision: RevisionRef,
    pub declared_intent: Vec<IntentSnippet>,
    pub capability_clusters: Vec<CapabilityCluster>,
    pub public_surface_delta: PublicSurfaceDelta,
    pub code_delta_summary: CodeDeltaSummary,
    pub changed_test_intent: Vec<TestIntentSummary>,
    pub behavior_delta: Option<BehaviorDeltaSummary>,
    pub neighboring_requirements: Vec<RequirementSummary>,
    pub quality_heuristics: Vec<HeuristicHint>,
}
```

Agent output:

```text
CandidateRequirements
CandidateScenarios
CandidateAcceptanceCriteria
QuestionsForQA
QuestionsForProduct
```

No candidate is sealed automatically.

---

# 66. Mandatory QA Verification state machine

```text
RECOVERED
   ↓
PROPOSED
   ↓
AUTO_CHECKED
   ↓
QA_REVIEW
   ├───────────────────┐
   ▼                   ▼
QA_VERIFIED    PRODUCT_DECISION_REQUIRED
   │                   │
   │                   ▼
   │             PRODUCT_APPROVED
   │                   │
   └─────────┬─────────┘
             ▼
        SEAL_ELIGIBLE
             ↓
           SEALED
```

QA verification is mandatory for spec recovered from implementation.

## 66.1 QA review workspace

For each candidate, show:

```text
candidate requirement
candidate acceptance criteria
why WVQ proposed it
declared PR/task evidence
base/head code impact
changed flows
behavior before/after
changed tests
coverage before/after
neighboring active requirements
missing edge cases
contradictions
oracle-independence warning
```

Example:

```text
Requirement:
Overflow values SHALL be represented by an Others node.

Why proposed:
✓ PR title names visual limit
✓ buildSankeyData changed
✓ groupOverflow added
✓ /others endpoint added
✓ new UI test expects Others

Observed:
base: no Others node
head: Others node when cardinality > limit

Uncertain:
? Viewer may access detail endpoint
? behavior during live refresh
? behavior exactly at visualLimit
```

QA reviews evidence, not merely an AI-written sentence.

## 66.2 QA actions

```text
ACCEPT AS INTENDED
EDIT
REJECT
OBSERVED ONLY
ADD SCENARIO
MARK DUPLICATE
MARK NON-BEHAVIORAL
REQUEST PRODUCT DECISION
REQUEST DEVELOPER CLARIFICATION
```

`OBSERVED ONLY` is important:

> “The implementation currently behaves this way, but QA cannot confirm that this behavior is intended.”

Observed-only behavior may become baseline evidence but cannot become a normative OracleSeal.

## 66.3 Adaptive QA checklist

WVQ asks only relevant questions.

Core:

```text
[ ] actor/role explicit?
[ ] precondition explicit?
[ ] trigger/action explicit?
[ ] expected result observable?
[ ] expectation independent of implementation detail?
[ ] boundaries represented?
[ ] error behavior represented where relevant?
[ ] permissions represented where relevant?
[ ] async/refresh behavior represented where relevant?
[ ] contradiction with existing requirement?
[ ] observed behavior differs from proposed intent?
[ ] new test appears to copy implementation behavior?
```

Numeric limit automatically adds:

```text
below?
exactly?
above?
min?
max?
```

Permissions add:

```text
Admin?
Operator?
Viewer?
tenant mismatch?
```

Async UI adds:

```text
loading?
refresh?
slow response?
failure?
double action?
```

## 66.4 Deterministic checks before QA review

WVQ performs:

```text
duplicate requirement detection
internal contradiction detection
code/spec contradiction
behavior/spec contradiction
testability check
implementation leakage check
missing actor/precondition
missing obvious boundary case
oracle-independence analysis
```

Examples of rejected weak criteria:

```text
"should work correctly"
"UI should look good"
"response should be fast"
```

unless a measurable oracle is added.

If expected behavior is supported only by new implementation + new test, display:

```text
ORACLE_INDEPENDENCE = WEAK
```

prominently.

## 66.5 Human verification evidence

```rust
pub struct HumanVerification {
    pub id: HumanDecisionId,
    pub reviewer: HumanIdentity,
    pub role: HumanRole,
    pub artifact_digest: ContentHash,
    pub decision: VerificationDecision,
    pub comment: Option<String>,
    pub timestamp: Timestamp,
}
```

Seal approval:

```rust
pub struct SealApproval {
    pub qa_verification: HumanDecisionId,
    pub product_approval: Option<HumanDecisionId>,
}
```

Product approval becomes mandatory when:

```text
QA requests product decision
candidate conflicts with declared intent
normative expected behavior is reconstructed only from implementation
a previously sealed expected result is being changed
```

---

# 67. Coverage Continuity and Protection Delta

This is a first-class subsystem, not an ordinary coverage report.

The critical question is not:

> “What is test coverage after the PR?”

It is:

> **“Which behavior/code flows were protected before the PR, which tests provided that protection, and did the PR preserve, improve, replace or accidentally remove that protection?”**

A PR may raise global line coverage while destroying the only test that protected an important old flow. WVQ must detect that.

Subsystem:

```text
wvq-protection
```

Core artifacts:

```text
BaseProtectionSnapshot
HeadProtectionSnapshot
CoverageContinuity
ProtectionDelta
TestLineage
FlowCoverageMatrix
```

---

# 68. Never compute impact only on head

This is a hard rule.

If a PR deletes:

```text
function
edge
route
branch
handler
test
```

the head graph no longer contains the old path.

Therefore:

```text
ImpactedSurface =
    Impact(base)
  ∪ Impact(head)
  ∪ RemovedNodes
  ∪ RemovedEdges
  ∪ RemovedPublicSurfaces
```

Store separately:

```text
base_only_impact
head_only_impact
shared_impact
removed_flow
new_flow
rewired_flow
```

This is essential for regression protection.

Example:

```text
base:
UI → controller → validation → service → endpoint

head:
UI → service → endpoint
```

Looking only at head cannot tell us that the validation step disappeared.

WVQ must preserve the base flow and test protection evidence for comparison.

---

# 69. Flow Impact Graph

For each change, WVQ derives impacted flows using Weavatrix.

A flow may start from:

```text
UI route/component interaction
HTTP endpoint
GraphQL operation
gRPC operation
event consumer/producer
public Go/JS API
scheduled/background entry point when discoverable
```

and connect through:

```text
component
handler
service
validation
domain logic
data access
outbound API/event
response
UI state/update
```

The exact graph evidence stays in Weavatrix. WVQ stores a flow projection.

```rust
pub struct ImpactedFlow {
    pub id: FlowId,
    pub revision: RevisionRef,
    pub entry: FlowEntry,
    pub graph_nodes: Vec<GraphNodeRef>,
    pub graph_edges: Vec<GraphEdgeRef>,
    pub public_surfaces: Vec<PublicSurfaceRef>,
    pub requirements: Vec<RequirementRef>,
}
```

---

# 70. Flow fingerprint and lineage

A flow needs continuity across revisions even when implementation is refactored.

```rust
pub struct FlowFingerprint {
    pub entry_surface: StableSurfaceRef,
    pub capability: Option<CapabilityRef>,
    pub observable_contract: Vec<ContractAtom>,
    pub structural_digest: ContentHash,
}
```

Matching across base/head should prefer:

```text
same endpoint/route/operation
same OpenSpec requirement
stable public symbol
stable component identity
Git rename/history evidence
graph-neighborhood similarity
semantic symbol fingerprint
```

Do not rely only on file paths.

States:

```text
UNCHANGED
MODIFIED
REWIRED
SPLIT
MERGED
ADDED
REMOVED
UNMATCHED
```

---

# 71. Test lineage

WVQ must understand tests across base/head.

```rust
pub enum TestLineageState {
    Unchanged,
    Modified,
    Renamed,
    Split,
    Merged,
    Added,
    Removed,
    Unmatched,
}
```

Identify using:

```text
stable test ID/name
Git rename/history
file similarity
test body fingerprint
covered obligations
covered graph regions
covered behavior states
```

Important distinction:

```text
same test file/name
≠
same protection
```

A test can still exist but stop reaching the same production path.

Therefore test lineage stores **dynamic protection lineage**, not only source lineage.

---

# 72. BaseProtectionSnapshot

Before evaluating head, build a snapshot for the affected base surface.

```rust
pub struct ProtectionSnapshot {
    pub revision: RevisionRef,
    pub affected_flows: Vec<FlowProtection>,
    pub affected_nodes: Vec<NodeProtection>,
    pub test_lineage: Vec<TestProtection>,
    pub obligations: Vec<ObligationProtection>,
}
```

For each affected flow:

```rust
pub struct FlowProtection {
    pub flow: FlowId,
    pub measured_tests: Vec<TestCaseRef>,
    pub recorded_sessions: Vec<SessionRef>,
    pub covered_nodes: CoverageBitmap,
    pub covered_branches: CoverageBitmap,
    pub proven_obligations: Vec<ObligationRef>,
    pub last_successful_proofs: Vec<ProofId>,
}
```

This answers:

```text
Which tests protected this flow before?
Which code in the flow did they actually execute?
Which requirement/scenario did they prove?
When was the last passing proof?
```

---

# 73. HeadProtectionSnapshot

Build the same representation for head after selected tests execute.

Do not compare just repository-wide percentages.

Compare per:

```text
impacted flow
impacted graph node
impacted requirement/scenario
public API
permission path
historically protected branch
```

---

# 74. ProtectionDelta categories

```rust
pub enum ProtectionDeltaState {
    Preserved,
    Improved,
    Degraded,
    Lost,
    Replaced,
    Relocated,
    NewUnprotected,
    ObsoleteRemoved,
    Unknown,
}
```

### PRESERVED

The same behavioral obligation remains protected with equivalent or stronger runtime evidence.

### IMPROVED

Head preserves old protection and adds meaningful coverage/proof.

### DEGRADED

Flow remains protected but fewer branches/nodes/scenarios are exercised.

### LOST

Base had measured/proven protection; head no longer has a valid proof path.

This is a critical regression signal even if global coverage rises.

### REPLACED

Old test disappears but another test/program/session proves the same obligation and impacted flow.

This can be healthy.

### RELOCATED

Refactor moved the implementation; test protection follows the semantically matched flow.

### NEW_UNPROTECTED

New behavior/code has no protection.

### OBSOLETE_REMOVED

Protection disappears because the corresponding OpenSpec behavior and flow are intentionally removed.

This is acceptable only with matching spec/intent evidence.

### UNKNOWN

WVQ cannot establish continuity and must not guess.

---

# 75. Coverage Continuity Matrix

For each impacted flow:

| Flow | Base tests | Head tests | Base coverage | Head coverage | Base Proof | Head Proof | State |
|---|---:|---:|---:|---:|---|---|---|
| Sankey render | 4 | 5 | 87% branches | 91% | PROVEN | PROVEN | IMPROVED |
| Others detail | 0 | 2 | — | 83% | UNPROVEN | PROVEN | IMPROVED |
| Viewer access | 2 | 1 | 100% critical branch | 0% critical branch | PROVEN | UNPROVEN | **LOST** |
| old endpoint | 2 | 0 | 78% | removed | PROVEN | removed | OBSOLETE_REMOVED |

The last state is safe only if OpenSpec intentionally removes that behavior.

---

# 76. Coverage numbers to compare

WVQ should prefer affected-surface measurements.

For each impacted unit:

```text
line coverage
branch coverage
function coverage where meaningful
executed graph nodes
executed critical edges
behavior-state coverage
requirement/scenario Proof coverage
API-operation coverage
permission-case coverage
```

Global coverage remains informational.

A global improvement must never override a local protection loss.

---

# 77. “Good after coverage” is insufficient

Example:

```text
Base global coverage: 76%
Head global coverage: 82%
```

looks positive.

But:

```text
Base:
Viewer delete permission denial branch = covered

Head:
Viewer delete denial branch = uncovered
```

WVQ verdict:

```text
GLOBAL_COVERAGE: IMPROVED
PROTECTION_CONTINUITY: DEGRADED
CRITICAL_PROTECTION_LOSS: Viewer delete authorization
```

The critical finding wins.

---

# 78. Test protection continuity checks

### WVQ-PROTECT-001 — previously protected flow lost

Base has successful measured Proof for a still-active obligation; head has none.

Default: `ERROR` for high/critical obligation.

### WVQ-PROTECT-002 — test exists but protection disappeared

Test survives by name/source, but no longer executes the affected flow/branch.

Default: `WARN/ERROR`.

### WVQ-PROTECT-003 — deleted test without equivalent replacement

Removed test was the only Proof path for an active obligation.

Default: `ERROR`.

### WVQ-PROTECT-004 — protection successfully replaced

Old test removed, new test covers same obligation and flow with equivalent/stronger evidence.

State: `FIXED/REPLACED`, no warning.

### WVQ-PROTECT-005 — changed test weakens oracle

Test changed together with implementation and removes/weakens a sealed assertion.

Default: `ERROR` unless new OracleSeal.

### WVQ-PROTECT-006 — coverage moved away from critical branch

Overall impacted coverage may stay high but critical branch/edge loses dynamic execution.

Default: risk-dependent `ERROR`.

### WVQ-PROTECT-007 — previously proven requirement becomes partial

Base `PROVEN`, head `PARTIAL/UNPROVEN`.

Default: `ERROR` for still-active requirement.

### WVQ-PROTECT-008 — old flow intentionally removed

Flow/test disappear together with approved OpenSpec removal.

State: `OBSOLETE_REMOVED`.

### WVQ-PROTECT-009 — new flow lacks protection

New/rewired flow has no relevant dynamic proof.

Default: `WARN/ERROR` by risk.

### WVQ-PROTECT-010 — suspicious coverage substitution

Head gains many newly covered low-risk lines while losing small but high-risk previously covered path.

Default: `WARN/ERROR`; explain exact surfaces.

---

# 79. Test-change risk

Tests changed in the same PR are not automatically bad, but they require special analysis.

Classify:

```text
implementation-only change
test-only change
implementation + test change
test deletion
test assertion weakening
test assertion strengthening
test setup-only modification
```

For implementation + test changes, compare base oracle and head oracle.

Important:

```text
test was modified to pass changed implementation
```

is not sufficient evidence that behavior is intended.

Check against:

```text
OpenSpec delta
OracleSeal delta
QA verification
```

If none exists:

```text
WVQ-PROTECT-011 POSSIBLE_TEST_ADAPTATION_TO_IMPLEMENTATION
```

This is exactly where recovered-spec QA verification is valuable.

---

# 80. Before-test evidence as a first-class safety baseline

For every PR, WVQ should answer:

```text
Before this change:
- what flows were protected?
- by which exact tests?
- what code/branches did they execute?
- what requirements did they prove?
- what was the latest passing revision?
```

Then:

```text
After this change:
- which protections are preserved?
- which moved?
- which strengthened?
- which disappeared?
- which new behaviors remain uncovered?
```

This is more useful than a single “coverage delta”.

---

# 81. Base-first execution strategy

Not every base test needs to be re-run for every PR.

Use historical stored measured evidence when it is:

```text
revision-bound
recent enough by policy
compatible with same test/program identity
compatible with same relevant dependency/environment fingerprint
```

If historical evidence is insufficient, run selected base tests.

Decision:

```text
stored trusted base Proof?
    yes → reuse
    no  → run minimal base selection
```

This controls CI cost while preserving correctness.

---

# 82. Coverage history

Store protection over time.

For a flow:

```text
revision A: 3 tests, PROVEN
revision B: 3 tests, PROVEN
revision C: 2 tests, PROVEN
revision D: 2 tests, PARTIAL
revision E: 1 test, UNPROVEN
```

WVQ can flag a gradual erosion before total loss.

New finding:

### WVQ-PROTECT-012 — protection erosion trend

Protection strength declines across several revisions even if no single PR crosses a hard threshold.

Default: `WARN`.

---

# 83. Flow-aware minimal test selection

Selection should use both base and head protection.

Candidate set:

```text
tests historically protecting base impacted flows
UNION
tests statically selected on head
UNION
tests dynamically covering head affected nodes
UNION
tests proving changed OpenSpec obligations
UNION
clone-sibling/risk-required tests
```

Then weighted set cover.

This prevents a new head-only selection algorithm from forgetting tests whose importance was visible only before the change.

---

# 84. Coverage-aware Spec Recovery

Recovered acceptance criteria should also use protection evidence.

Example:

```text
PR changes permission code.
Base tests show:
- Admin allow
- Viewer deny

Head changed test keeps:
- Admin allow
but Viewer deny test disappeared.
```

Spec Recovery should propose:

```text
Candidate acceptance criterion:
Viewer SHALL remain unable to perform X.

Evidence:
- historically protected behavior
- base measured test
- permission graph path
- no declared SpecDelta removing it
```

But because this is recovered from historical implementation/testing rather than declared product intent, QA must verify it before sealing.

This turns old tests into **intent clues**, not automatic truth.

---

# 85. Coverage Continuity in QA Review

When QA reviews recovered requirements, show:

```text
BASE PROTECTION
✓ Viewer denied — test auth-viewer.spec
✓ branch coverage 100% on permission guard
✓ last passing Proof P-811

HEAD
⚠ test removed
⚠ guard branch no longer measured
? OpenSpec has no permission change

Question:
Should Viewer remain denied?

[Yes — add acceptance criterion]
[No — intentional change]
[Ask PM]
```

This is much faster and safer than asking QA to rediscover the old behavior manually.

---

# 86. New domain types

```rust
pub struct ProtectionSnapshot {
    pub revision: RevisionRef,
    pub flows: Vec<FlowProtection>,
    pub tests: Vec<TestProtection>,
    pub obligations: Vec<ObligationProtection>,
}

pub struct FlowProtection {
    pub flow: FlowId,
    pub tests: Vec<TestCaseRef>,
    pub sessions: Vec<SessionRef>,
    pub covered_nodes: CoverageBitmap,
    pub covered_branches: CoverageBitmap,
    pub proofs: Vec<ProofId>,
}

pub struct TestProtection {
    pub test: TestCaseRef,
    pub lineage: TestLineageState,
    pub flows: Vec<FlowId>,
    pub obligations: Vec<ObligationRef>,
    pub measured_nodes: CoverageBitmap,
}

pub struct ProtectionDelta {
    pub flow: FlowId,
    pub state: ProtectionDeltaState,
    pub base: Option<FlowProtectionRef>,
    pub head: Option<FlowProtectionRef>,
    pub reasons: Vec<ProtectionReason>,
}
```

---

# 87. MCP additions for protection analysis

Keep default surface compact.

Add to advanced/QA profile:

```text
quality_protection
quality_test_lineage
quality_flow
```

### `quality_protection`

Input:

```json
{
  "base": "main",
  "head": "HEAD",
  "scope": "impacted"
}
```

Output:

```text
preserved
improved
degraded
lost
replaced
new_unprotected
critical findings
```

### `quality_test_lineage`

Explains what happened to a test and whether its protection changed.

### `quality_flow`

Returns one bounded impacted flow:

```text
base path
head path
requirements
tests before
tests after
coverage before
coverage after
Proof before
Proof after
```

---

# 88. Updated `quality_verify` output

Add:

```text
FLOW IMPACT
PROTECTION CONTINUITY
```

Example:

```text
FLOW IMPACT
  affected flows          12
  unchanged                5
  rewired                  4
  new                      2
  removed                  1

PROTECTION CONTINUITY
  preserved                7
  improved                 3
  degraded                 1
  lost                     1

CRITICAL
  Viewer authorization flow:
    base: PROVEN by T-77
    head: UNPROVEN
    branch coverage: 100% → 0%
    no matching OpenSpec permission change

VERDICT
  BLOCKED
```

This is more meaningful than:

```text
coverage 76% → 82%
```

---

# 89. New implementation tasks — Spec Recovery

## Task 24 — Recovery evidence model

**Create**
- `crates/wvq-spec-recovery/Cargo.toml`
- `src/{lib,evidence,narrative,cluster}.rs`

**Tests**
- commit title alone cannot produce verified intent;
- declared PR criterion outranks observed code;
- implementation + changed test alone gives weak oracle independence.

**Commit**
```bash
git commit -m "feat(spec-recovery): model intent evidence"
```

## Task 25 — PR/commit clustering

- [ ] Cluster multi-commit implementation into capability changes.
- [ ] Prefer OpenSpec/API/community evidence over title similarity.
- [ ] Preserve commit provenance.
- [ ] Commit `feat(spec-recovery): cluster implementation into capability changes`.

## Task 26 — deterministic candidate verifier

Checks:

```text
duplicates
contradictions
non-testable wording
implementation leakage
missing actor/precondition
missing boundary/negative cases
code/behavior contradiction
oracle independence
```

- [ ] Commit `feat(spec-recovery): verify candidate acceptance criteria`.

## Task 27 — mandatory QA review state machine

Tests:

```text
recovered cannot seal without QA
OBSERVED_ONLY cannot become normative
REQUEST_PRODUCT_DECISION blocks seal
QA edit invalidates old digest
product approval resolves escalation
```

- [ ] Commit `feat(spec-recovery): require QA verification`.

## Task 28 — spec-recovery MCP + Studio

MCP profile:

```text
quality_spec_recover
quality_spec_review
quality_spec_questions
quality_spec_preview_patch
quality_spec_verify
quality_spec_seal
```

Studio:

```text
candidate requirement
evidence
observed vs intended
acceptance criteria
coverage-before/after
missing cases
QA actions
product escalation
patch preview
```

- [ ] Commit `feat(spec-recovery): add reviewed recovery workflow`.

---

# 90. New implementation tasks — Coverage Continuity

## Task 29 — dual-revision impacted surface

**Files**
- Create `crates/wvq-intelligence/src/flow.rs`
- Create `crates/wvq-intelligence/src/impact_union.rs`
- Test `tests/dual_revision_impact.rs`

**Tests**
- removed base edge remains in impacted surface;
- removed endpoint is represented;
- head-only new flow represented;
- shared flow matched across refactor.

**Required algorithm**

```text
Impact(base)
∪ Impact(head)
∪ graph_diff.removed_nodes
∪ graph_diff.removed_edges
∪ public_surface_delta.removed
```

**Commit**
```bash
git commit -m "feat(impact): preserve base and head affected flows"
```

## Task 30 — Test lineage

**Files**
- `wvq-intelligence/src/test_lineage.rs`
- tests `test_lineage.rs`

**Tests**
- unchanged name/source;
- Git rename;
- modified test;
- removed test;
- same test source but changed dynamic flow coverage;
- split/merged candidates.

**Commit**
```bash
git commit -m "feat(protection): track test lineage across revisions"
```

## Task 31 — ProtectionSnapshot

**Files**
- Create `crates/wvq-proof/src/protection.rs`
- Store migration for flow/test protection.

**Tests**
- base flow stores exact tests + dynamic coverage + Proof;
- stale/non-revision-bound evidence rejected;
- historical trusted Proof can be reused by policy.

**Commit**
```bash
git commit -m "feat(protection): snapshot runtime protection by revision"
```

## Task 32 — ProtectionDelta

**Tests**
- preserved;
- improved;
- degraded;
- lost;
- replaced;
- relocated;
- new unprotected;
- intentional obsolete removal;
- unknown.

**Critical test**

```text
global coverage increases
but critical base branch loses all protection
→ ProtectionDelta::Lost
```

**Commit**
```bash
git commit -m "feat(protection): compare base and head safety nets"
```

## Task 33 — Flow-aware selection

Selection candidates must include:

```text
base historical protectors
head static selectors
head dynamic protectors
obligation tests
risk-required tests
```

- [ ] Test that a base-only historically important test remains selected after a head graph edge is removed.
- [ ] Commit `feat(selection): preserve historical regression protection`.

## Task 34 — Protection checks

Implement:

```text
WVQ-PROTECT-001 ... WVQ-PROTECT-012
```

- [ ] Keep global coverage improvement from suppressing local protection loss.
- [ ] Detect test assertion weakening against same OracleSeal.
- [ ] Treat approved removed behavior as `OBSOLETE_REMOVED`.
- [ ] Commit `feat(checks): gate protection continuity regressions`.

## Task 35 — Protection MCP and UI

MCP advanced profile:

```text
quality_protection
quality_test_lineage
quality_flow
```

Studio:

```text
Flow Impact
Coverage Before
Coverage After
Tests Before
Tests After
Proof Before
Proof After
Protection Delta
```

- [ ] QA can answer “what protected this before?” in one view.
- [ ] Commit `feat(studio): explain test protection continuity`.

---

# 91. Updated product invariant

The complete safety chain is now:

```text
PR / commits / code
      ↓
Spec Recovery
      ↓
Candidate OpenSpec + acceptance criteria
      ↓
Mandatory QA Verification
      ↓
OracleSeal
      ↓
           ┌────────────────────────────┐
           │                            │
           ▼                            ▼
   Weavatrix base graph          Weavatrix head graph
           │                            │
           └────────────┬───────────────┘
                        ▼
               Dual-Revision Impact
                        │
                        ▼
                   Flow Lineage
                        │
             ┌──────────┴──────────┐
             ▼                     ▼
   BaseProtectionSnapshot   HeadProtectionSnapshot
             │                     │
             └──────────┬──────────┘
                        ▼
                 ProtectionDelta
                        │
                        ▼
                Quality Debt Ratchet
                        │
                        ▼
                Minimal TestProgram
                        │
                        ▼
              deterministic execution
                        │
                        ▼
                   BehaviorDelta
                        │
                        ▼
                      Proof
                        │
                        ▼
                  QualityVerdict
                        │
                        ▼
                exception-only QA
```

The crucial rule:

> **Head coverage is not enough. WVQ must preserve and explain the test protection that existed before the change.**

A change is not “safer” merely because it has more covered lines after implementation. It is safer only when:

```text
old required protections remain valid
AND changed/new behavior gains appropriate proof
AND critical old flows are not silently de-protected
AND any removed protection corresponds to intentionally removed behavior
```

This makes `weavatrix-quality` a protection-continuity system rather than a prettier coverage dashboard.

---

# Part III — Whole-conversation synthesis, ecosystem decisions, and final product strategy

The sections below capture the material that existed in earlier research/discussion but was not fully preserved in the v2 implementation plan. They are normative where they define build-vs-integrate boundaries and strategic priorities.

# 92. Final product identity

`weavatrix-quality` is **not**:

```text
another Playwright wrapper
another test runner
another browser MCP
another code-coverage dashboard
another LLM agent that clicks through a UI
another static-analysis linter
```

It is:

> **A revision-bound Quality Protection System: spec-to-proof compilation + protection continuity + change-aware quality ratcheting.**

The product’s most compact statement is:

```text
OpenSpec says what should remain true.
Weavatrix says what changed and what it can affect.
Existing runners execute the smallest relevant protection set.
WVQ proves whether old protection survived and new behavior gained proof.
Humans review only unresolved product intent.
```

A stronger positioning than “AI testing” is:

> **Weavatrix Quality turns product intent and repository change into executable, revision-bound proof while preserving the safety net that existed before the change.**

This wording matters. The durable moat is not AI generation; generation will commoditize. The moat is the persistent relationship between:

```text
intent
code graph
flow lineage
test lineage
runtime behavior
historical protection
quality debt
evidence
proof
```

---

# 93. The organizational outcome: replace routine QA work, not product judgment

The desired operating model is intentionally aggressive.

## Traditional manual QA work that WVQ should make mostly unnecessary

```text
repeat known regression flows
open the same pages after every PR
manually re-check unchanged permissions
manually gather console/network/screenshots
manually write deterministic reproduction steps
manually retest the same fixed bug
manually compare “before vs after”
manually run broad smoke/regression suites
manually discover that a known test disappeared
manually inspect hundreds of green cases
```

These activities should become machine work.

## Manual QA evolves into Quality Analyst

Primary responsibilities:

```text
verify recovered/ambiguous product intent
decide novel UX behavior
identify missing product risks
perform genuinely novel exploratory testing
approve/reject recovered acceptance criteria
approve intentional behavior/baseline changes
review rare HUMAN_REQUIRED cases
```

The Quality Analyst should reason about **intent and risk**, not execute a script that a machine already knows.

## Automation QA evolves into Quality Platform Engineer

Primary responsibilities:

```text
runner adapters
test environment determinism
fixtures/test-data systems
network virtualization
coverage fidelity
browser instrumentation
mutation operators
flake policies
CI quality gates
quality policy calibration
executor reliability
```

Automation QA becomes more valuable, not less: they own the platform that lets a small organization operate with far less repetitive manual regression labor.

## Product target

The aspiration is not “fire all manual QA”. The technical target is stronger and more useful:

> **Make routine manual regression and routine failure triage unnecessary enough that existing QA capacity can absorb much higher development throughput.**

If, after deployment, a QA engineer spends most of the day replaying known flows, WVQ has failed.

---

# 94. Research-landscape provenance from the earlier large scan

The original landscape exercise considered three large popularity universes:

```text
GitHub: top ~5,000 repositories by stars at the research snapshot
PyPI:   top ~5,000 packages by download ranking
npm:    top ~5,000 popular/download-heavy packages from a reproducible ranking proxy
```

The goal was not to claim license/feature metadata was exhaustively hand-verified for every one of 15,000 positions. The methodology was:

1. define a large popularity universe;
2. isolate testing/UI/automation-relevant candidates;
3. deep-inspect the strongest candidate families;
4. filter direct-source-port opportunities primarily to MIT/Apache-2.0;
5. compare with mature Rust/Cargo alternatives;
6. classify whether the opportunity should be **built, integrated, adapted, clean-room implemented, or deliberately skipped**.

The most important outcome of that research was a strategic correction:

> **Rust is missing product-level cross-language quality intelligence much more than it is missing low-level test primitives.**

Rust already has strong test runners, coverage, mutation, mocking, property testing, load testing and browser-driver primitives. Therefore the product should not spend its engineering budget recreating them.

---

# 95. Build / borrow / integrate matrix

| Area | Decision | Reason |
|---|---|---|
| Code graph / impact | **Embed Weavatrix Rust** | existing unique advantage; do not duplicate |
| Browser engine | **Use Playwright** | browser rendering dominates; replacement has terrible ROI |
| Browser coding-agent CLI | **Use/learn from Playwright CLI** | current official guidance favors CLI+skills for token-efficient coding agents |
| Generic browser MCP | **Do not build** | official Playwright MCP already exists; commodity surface |
| Component runner | **Integrate Storybook/Vitest where present** | modern real-browser component testing already exists |
| Semantic UI targeting | **Adopt Testing Library principles in TestProgram targets** | semantic/a11y identity is more stable than CSS/XPath |
| Network mocking | **Adapter to project-native/MSW; later WVQ network IR** | MSW already solves network-level mocking well |
| Rust test runner | **Use cargo-nextest / cargo test** | mature ecosystem |
| Rust coverage | **Use cargo-llvm-cov** | precise mature coverage; no need to port coverage.py |
| Rust snapshots | **Use insta when needed** | mature |
| Rust mutation | **Use/learn from cargo-mutants plus WVQ targeted mutation** | primitive exists; WVQ adds requirement/impact targeting |
| Rust HTTP mocks | **Use wiremock/httpmock/mockito ecosystem** | already mature |
| Rust load testing | **Use Goose** | no reason to port Locust |
| Property testing | **Use existing Rust libraries** | proptest/quickcheck/arbitrary ecosystem already covers primitive |
| Browser/session intelligence | **Build WVQ BehaviorGraph/ProtectionDelta** | this is product-level gap/moat |
| Cross-language Proof ledger | **Build** | mature equivalent not found |
| Impact-based minimal regression | **Build** | combines unique Weavatrix evidence with runtime protection |
| Protection continuity | **Build** | key differentiator; head-only tools miss it |
| Spec Recovery + OracleSeal | **Build** | solves brownfield/oracle-bias problem |
| Flake root-cause memory | **Build** | cross-run graph-grounded layer is differentiating |
| Visual baseline governance | **Build product layer, reuse diff kernels** | approval/history/policy matter more than raw pixel math |
| Full JS DOM/web runtime in Rust | **Defer** | huge effort; only build subsets if profiling proves need |
| Full Storybook clone in Rust | **Do not build initially** | adapter/portable-story support is higher ROI |
| Full Cypress clone | **Do not build** | no moat |
| Generic AI multi-agent swarm | **Do not build** | expensive, nondeterministic, duplicates Cortex/agents |

---

# 96. Why Playwright MCP is not the product moat

The official Playwright MCP provides browser automation through structured accessibility snapshots. Current Playwright guidance explicitly distinguishes:

```text
CLI + skills
    better for high-throughput coding-agent work
    lower schema/context overhead

MCP
    useful for persistent browser state,
    exploratory loops,
    rich page introspection
```

Therefore WVQ should not expose dozens of tools such as:

```text
browser_click
browser_type
browser_hover
browser_go_back
browser_screenshot
...
```

as its primary MCP.

WVQ’s MCP is high-level:

```text
quality_context
quality_plan
quality_run
quality_status
quality_verify
quality_explain
quality_evidence
```

and optional QA/protection tools:

```text
quality_select
quality_protection
quality_test_lineage
quality_flow
quality_replay
quality_explore
quality_mutate
quality_debt
quality_spec_recover
...
```

The browser is an executor hidden behind typed plans, not the ontology presented to the coding agent.

This is both a product-boundary decision and a token-economics decision.

---

# 97. Frontend testing strategy: component → flow → full E2E

UI-heavy systems should not send every change directly to full E2E.

WVQ should maintain three execution granularities.

## 97.1 Component protection

When a changed flow is local to a component/story:

```text
Storybook story / portable story
→ Vitest browser mode / Playwright Chromium
→ interaction assertion
→ a11y/visual checks
```

Modern Storybook can transform stories into browser component tests and run them through Vitest browser mode using Playwright Chromium. WVQ should consume those assets instead of inventing another component format immediately.

WVQ value:

```text
which stories/components are impacted?
which old story states protected this component?
which OpenSpec scenarios map here?
did protection survive the PR?
```

## 97.2 Feature-flow protection

Use `TestProgram` for multi-component behavior:

```text
route
→ semantic interaction
→ API behavior
→ resulting state
```

## 97.3 Full E2E

Reserve expensive full E2E flows for:

```text
cross-domain behavior
critical auth/permission flows
real integration boundaries
release gates
behavior not faithfully representable below
```

This hierarchy reduces browser minutes and feedback latency without sacrificing critical protection.

---

# 98. Testing Library semantics become a TestProgram design rule

Testing Library recommends querying interfaces in a way that resembles how a user perceives them, prioritizing semantic queries over implementation classes/IDs.

WVQ therefore treats locator mechanics as a projection of a semantic target:

```rust
Target {
    role,
    accessible_name,
    label,
    stable_test_id,
    component_hint,
    scope,
    fallback_css,
}
```

The semantic identity is what persists across revisions.

A CSS selector is not the durable test identity.

Benefits:

```text
less selector churn
better safe healing
natural accessibility checks
better behavior-state matching
lower coupling to component implementation
more meaningful recorder output
```

---

# 99. Storybook/Vitest should become a first-class adapter, not a WVQ clone

Add a later adapter:

```text
wvq-storybook
```

Responsibilities:

```text
discover stories / portable stories
map story → component graph nodes
map story → OpenSpec obligation
select impacted stories
run Storybook/Vitest browser tests
collect story coverage
collect a11y/visual/component evidence
preserve story protection across revisions
```

Important opportunity:

```text
changed React/Vue/Svelte component
→ Weavatrix component impact
→ select minimal Storybook states
→ run in real browser
→ only escalate to E2E if component-level Proof is insufficient
```

This can save more time than optimizing Playwright command latency.

---

# 100. Network virtualization and deterministic replay

Network nondeterminism is a major source of slow/flaky UI tests.

MSW demonstrates an important pattern: intercept network requests at the network boundary and reuse mock definitions across development/testing contexts.

WVQ should support a runner-neutral `NetworkPolicy` / later `NetworkProgram`:

```rust
pub struct NetworkPolicy {
    pub mode: NetworkMode,
    pub replay: Option<ReplayProfile>,
    pub faults: Vec<NetworkFault>,
    pub redaction: RedactionPolicyRef,
}
```

Modes:

```text
live
record
replay
hybrid
mock
```

Faults:

```text
latency
timeout
connection reset
HTTP 4xx/5xx
malformed response
truncated response
retry-after
duplicate response/event
out-of-order response where applicable
auth expiry
rate limit
```

Use cases:

```text
record one trusted session
→ deterministic replay on every relevant PR
→ inject one targeted failure profile when risk says necessary
```

Do not use an LLM to improvise network failure conditions on every run.

---

# 101. Fault injection and chaos as a quality capability

Earlier testing research also identified a strong opportunity around failure injection / chaos rather than another conventional unit-test framework.

WVQ should treat fault injection as an execution capability, not necessarily implement every fault itself.

Canonical object:

```rust
pub struct FaultProfile {
    pub id: FaultProfileId,
    pub scope: FaultScope,
    pub triggers: Vec<FaultTrigger>,
    pub expected_obligations: Vec<ObligationRef>,
}
```

Potential adapters:

```text
network proxy
browser network interception
application test hooks
MCP chaos/fuzz harness for MCP products
container/service fault tools
```

For Node/Bun/Go services this supports deterministic tests such as:

```text
DB call slow
upstream 500
auth token expires mid-flow
response schema truncated
connection drops after side effect
duplicate event
retry race
```

Fault profiles should be selected by risk/impact, not run globally.

---

# 102. Accessibility strategy

Accessibility is particularly valuable because it doubles as:

```text
product quality signal
AND stable semantic interaction surface
```

WVQ should implement:

1. deterministic semantic-target checks;
2. a11y tree delta;
3. selected accessibility-rule adapter;
4. requirement-aware severity;
5. base/head a11y protection continuity.

Important licensing decision from the earlier scan:

- `axe-core` is MPL-2.0, so under a strict MIT/Apache-only **source-port** policy it should **not** be directly ported by copying source.
- WVQ may invoke an external compatible tool when product/legal policy permits, or independently implement standards-derived checks using WCAG/WAI-ARIA sources.
- Do not call a clean-room implementation “an axe-core port”.

High-value built-ins can begin with objectively testable rules:

```text
interactive control has accessible name
form control label association
duplicate ID where relevant
focusability/disabled consistency
role/state consistency
keyboard reachability for required flows
dialog name/focus behavior
```

---

# 103. Visual regression strategy

Do not make visual testing “send two full screenshots to a frontier vision model”.

Pipeline:

```text
base/head structured DOM+a11y+geometry
          ↓
native/cheap region diff
          ↓
pixel diff only on changed relevant region
          ↓
baseline/policy comparison
          ↓
vision model only when semantics remain ambiguous
```

The product value is not the image-diff kernel.

It is:

```text
which visual surface was impacted?
which baseline is authoritative?
which requirement/design intent applies?
was this region historically stable?
is the change expected by SpecDelta/Figma?
who approved the previous/current baseline?
```

Earlier strict-license scan note:

- `pixelmatch` is ISC, not MIT/Apache-2.0. Under strict direct-source-port rules, do not copy/port its implementation as a source transplant.
- A native image-diff algorithm can be independently implemented or another compatible dependency can be used according to product licensing policy.

---

# 104. Figma is a presentation oracle, not behavior authority

Later phase:

```text
Figma structured design context
          │
          ▼
presentation obligations
          │
          ├── component identity
          ├── token/spacing/type
          ├── state visibility
          ├── responsive expectations
          └── design annotations
```

Authority hierarchy remains:

```text
OpenSpec → intended product behavior
Figma    → intended presentation/design
runtime  → observed behavior
```

A Figma mismatch cannot silently rewrite an OpenSpec behavior oracle.

Use vision only when structured design/browser evidence cannot resolve a presentation delta.

---

# 105. Contract and API testing layer

A strong API layer reduces how much has to be discovered through UI.

WVQ should progressively add:

```text
OpenAPI
GraphQL
gRPC/protobuf
WebSocket where applicable
MCP protocol for agent products
event/message transports
```

Weavatrix already provides graph evidence for many of these surfaces.

Later contract-generation adapters can borrow ideas from systems such as Schemathesis without turning WVQ into a clone.

Canonical flow:

```text
SpecDelta
→ API/transport delta
→ impacted producer/consumer flows
→ existing contract Proof
→ generated boundary/negative cases
→ deterministic execution
→ Proof
```

Especially valuable for Node/Bun/Go backends.

---

# 106. Quality Coverage Tensor

A single line percentage cannot express the product surface.

WVQ should think in sparse coverage cells:

```text
Requirement/Scenario
× Actor/Role
× Route/Component/API
× Data class/boundary
× Feature flag
× Browser/runtime
× Environment/fault profile
× Behavior state
```

Example:

```text
R17-S2
× Viewer
× Sankey
× visualLimit=10
× cardinality=15
× Chromium
× normal network
```

Do **not** materialize a Cartesian explosion.

Use:

```text
risk
pairwise/combinatorial selection
boundaries
historical bugs
change impact
previous protection
mutation survivors
```

to materialize only valuable cells.

The tensor is a conceptual selection space; `ProtectionSnapshot` stores the cells/flows that are actually proven.

---

# 107. Spec Recovery is mandatory for brownfield adoption

Many real repositories will not begin with complete OpenSpec.

Therefore WVQ supports:

```text
PR/commits/code
→ deterministic ChangeNarrative
→ candidate capability clusters
→ candidate requirements/scenarios/acceptance criteria
→ deterministic sanity checks
→ mandatory QA verification
→ product escalation where necessary
→ OracleSeal
```

Evidence hierarchy:

```text
Tier A: declared product intent
Tier B: reviewed collaboration evidence
Tier C: implementation/runtime evidence
Tier D: weak naming hints
```

Critical rule:

> **Code and tests are evidence of observed behavior, not automatic proof of intended behavior.**

Old tests are especially useful as *intent clues* because they show what was deliberately protected in the past, but recovered expectations must still be reviewed before becoming normative.

This makes WVQ deployable incrementally:

```text
new changes become well-specified
old behavior is recovered only when touched
protection history gradually improves
no “document the entire legacy system first” project
```

---

# 108. Protection Continuity is P0, not a later dashboard feature

The whole product should optimize for this question:

> **What protected the affected flow before the PR, and what protects it now?**

This requires:

```text
DualRevisionImpact
FlowLineage
TestLineage
BaseProtectionSnapshot
HeadProtectionSnapshot
ProtectionDelta
```

The impacted surface is:

```text
Impact(base)
∪ Impact(head)
∪ removed nodes
∪ removed edges
∪ removed public surfaces
```

Never compute only from head.

A deleted validation edge or removed test may be invisible if the algorithm only looks at the final graph.

Priority finding:

```text
GLOBAL COVERAGE: 76% → 82%
CRITICAL FLOW: Viewer-deny branch 100% → 0%
PROTECTION: LOST
VERDICT: BLOCKED
```

The local protection loss wins.

This is one of the strongest differentiators from normal coverage tools.

---

# 109. Historical Protection Graph

Over time WVQ should build a persistent temporal protection graph:

```text
Requirement
   ↕
Flow
   ↕
Code regions
   ↕
Tests / sessions / TestPrograms
   ↕
Proofs
   ↕
Revisions
```

This enables:

```text
Which test used to protect this flow?
When did protection weaken?
Which PR introduced the first erosion?
Was protection replaced or merely lost?
Which requirements depend on this test?
Which tests are redundant?
Which single test is a dangerous single point of protection?
```

New useful finding families:

### Single protector

```text
critical obligation has exactly one valid dynamic protector
```

→ warn about fragile safety net.

### Correlated protectors

Several “different” tests execute essentially the same path and oracle; protection diversity is weaker than test count suggests.

### Protection erosion trend

Already defined in v2; extend with time series.

### Stale proof

Historical Proof is too old or environment/dependency fingerprint no longer compatible.

### Phantom test

Test exists and passes but no longer exercises the flow it is believed to protect.

These are higher-value QA signals than generic “number of tests”.

---

# 110. Existing Rust/Cargo ecosystem means WVQ should stay product-level

## cargo-nextest

Current nextest supports infrastructure-grade test execution features including:

```text
retries/flaky classification
record/replay/rerun
JUnit
partitioning/sharding
filtersets
per-test settings
CI profiles
```

WVQ should consume nextest output when testing Rust components instead of building a Rust runner.

## cargo-llvm-cov

Provides precise LLVM source-based coverage and integrates with cargo test/nextest. WVQ should ingest its LCOV/JSON artifacts if WVQ itself has Rust modules under test.

## wiremock-rs/httpmock/mockito

Rust HTTP mocking is already mature. WVQ’s differentiator is selecting/recording the right faults and connecting them to obligations/Proof, not reimplementing a basic mock server.

## Goose

Goose is a Rust load-testing system inspired by Locust and publishes significant per-core performance advantages. Therefore a direct Locust→Rust port was deprioritized in the earlier research.

## Other mature primitives

Examples include snapshot, mutation, property/fuzz, testcontainer and browser-driver crates. WVQ should integrate or normalize them when useful.

### Strategic conclusion

> **Do not create a Rust implementation merely because the existing tool is Python/JS. Create Rust where Rust enables a persistent low-overhead control plane, cross-language evidence model, fast graph math, deterministic selection or bounded MCP.**

---

# 111. Frontend ecosystem opportunities that remain real

After excluding commodity primitives, the strongest product-level gaps remain:

```text
cross-language frontend Test IR
requirement-aware component/story selection
deterministic record/replay with behavior lineage
cross-browser baseline governance
network replay/fault policy tied to requirements
unified UI evidence ledger
impact-aware UI/component/story/E2E selection
failure clustering + graph-backed root cause
protection continuity across refactors
safe high-level quality MCP
```

These are suitable WVQ territories.

A full `jsdom`-class web runtime or complete Storybook replacement remains a very high-effort later option and should only be revisited if profiling proves an executor/runtime bottleneck that adapters cannot solve.

---

# 112. Why not port Jest/Vitest/pytest/Robot/coverage.py/Locust wholesale

Earlier candidate review concluded these direct ports are low-value because WVQ does not win by owning runner syntax.

Examples:

```text
pytest/Robot/Jest/Vitest runner
    → normalize/adapt; do not rebuild first

coverage.py
    → use native language coverage sources; WVQ unifies evidence

Locust
    → Goose already provides strong Rust load testing

snapshot testing
    → mature Rust/JS primitives exist

mutation primitives
    → existing tools exist; WVQ adds impact/obligation targeting

HTTP mocking
    → mature ecosystem exists
```

The engineering budget belongs in cross-run, cross-language intelligence.

---

# 113. License guardrails inherited from the original MIT/Apache-focused research

The early scan deliberately emphasized MIT and Apache-2.0 source reuse.

Important direct-source-port exclusions discovered:

```text
axe-core   → MPL-2.0
pixelmatch → ISC
nyc        → ISC
mutmut     → BSD-3-Clause
```

This does **not** mean the algorithms/categories cannot exist in WVQ.

It means:

```text
do not copy/translate those source implementations
under a “MIT/Apache source-port” assumption
```

Acceptable alternatives depend on legal/product policy:

```text
invoke as external adapter
use a compatible differently licensed dependency
implement from public standards/specifications
perform clean-room independent implementation
```

Record provenance for copied/adapted code and keep the core’s license posture deliberate.

High-value sources that were compatible with the original MIT/Apache focus included families such as Playwright, Storybook, Testing Library, Vitest, MSW, pytest, Robot Framework, Schemathesis, coverage.py, mitmproxy, Locust and others; exact per-package licensing should still be rechecked at implementation time rather than assumed from an old snapshot.

---

# 114. Portfolio integration map

WVQ should be a separate repository/product but exploit existing ecosystem pieces through narrow interfaces.

## Weavatrix / weavatrix-rust — hard dependency

Owns:

```text
repository truth
revision identity
code graph
change impact
graph diff
test candidates
API/transport graph
architecture
health
history
coverage attachment
```

WVQ embeds the Rust engine, not the Weavatrix MCP, for its internal hot path.

## mcport — hard dependency for MCP host

Owns:

```text
fast typed MCP transport
bounded schemas
cancellation/deadlines
bounded queues
small server surface
```

## Cortex Loom — optional decision/context adapter

Owns:

```text
compact evidence packet
risk-aware model routing
local/frontier model selection
QA decision sequences
```

WVQ must remain fully functional without it.

Suggested Cortex sequences:

```text
QA_SPEC_REVIEW
QA_PLAN
QA_EXPLORE
QA_TRIAGE
QA_HEAL
QA_VERIFY
```

## BranchPilot — developer projection

BranchPilot is a natural place to show:

```text
OpenSpec linked
Protection preserved/lost
new debt
impacted tests
Proof verdict
baseline change requiring review
```

It should link into Quality Studio rather than own quality logic.

## Weavatrix Loom — later capability registry

Do not add to critical v1 path.

When executor/adaptor count becomes large, Loom can model capabilities such as:

```text
test.browser.execute
test.component.execute
test.network.replay
test.mutation.execute
```

with swappable implementations and conformance evidence.

## FerroSift — optional deterministic test-data transforms

Potential use:

```text
fixture transforms
encoding/decoding
deterministic redaction
payload mutation recipes
data preparation
```

Do not merge its operation registry into WVQ.

## SightLoom — later long-session/video intelligence

Potential use:

```text
long QA recording indexing
interesting interval detection
visual anomaly intervals
evidence reels
privacy/redaction intervals
```

Not a v1 dependency.

## ReelForge / existing visual kernels — optional executor

Where existing fast media/image diff primitives are suitable, reuse them behind WVQ’s evidence policy rather than create another image-processing library.

## GrantTap — optional remote quality control surface

Not a core dependency, but a future mobile/agent-control projection could show:

```text
PR quality verdict
HUMAN_REQUIRED decision
approve rerun
approve/reject baseline change
```

Quality truth remains in WVQ.

---

# 115. Model strategy

WVQ is model-neutral.

Recommended roles:

| Role | Preferred class |
|---|---|
| new requirement/spec reasoning | strong frontier agent |
| recovered-spec synthesis | strong agent, bounded packet |
| deterministic test execution | no model |
| normal regression | no model |
| known failure classification | no model |
| unknown failure semantic analysis | small/medium or frontier based on risk |
| browser tarpit escape | bounded agent call |
| visual ambiguity | multimodal model only after crop/diff |
| clustering/embeddings if needed | local/small |
| final high-risk oracle review | human + optional second model |

Author and verifier may use different models when an external orchestrator chooses to do so, but WVQ should never secretly create a nested paid-agent swarm.

---

# 116. Three execution modes

## Mode A — deterministic green path

```text
spec already sealed
known impacted flow
protection history exists
selected tests execute
no unexpected behavior delta
no new blocking debt
```

AI:

```text
0 runtime tokens
```

Result:

```text
AUTO_PROVEN
```

## Mode B — deterministic contradiction

```text
sealed oracle
head contradicts oracle
reproducible evidence
```

AI is optional for prose.

Result:

```text
CONTRADICTED / PRODUCT_REGRESSION
```

## Mode C — unresolved intent

```text
new/unmatched behavior
weak oracle independence
Spec Recovery ambiguity
protection lineage cannot be matched
```

Result:

```text
HUMAN_REQUIRED
```

An AI packet can assist, but it cannot silently manufacture normative truth.

---

# 117. Result taxonomy should remain explicit

Avoid generic:

```text
PASS
FAIL
```

for the whole product.

Use orthogonal axes.

## Behavioral Proof

```text
PROVEN
CONTRADICTED
PARTIAL
UNPROVEN
HUMAN_REQUIRED
```

## Protection continuity

```text
PRESERVED
IMPROVED
DEGRADED
LOST
REPLACED
RELOCATED
NEW_UNPROTECTED
OBSOLETE_REMOVED
UNKNOWN
```

## Quality debt

```text
NEW
EXISTING
FIXED
RETURNED
EXCEPTED
APPROACHING_BUDGET
```

## Runtime stability

```text
STABLE
KNOWN_FLAKY
NEW_FLAKY
ENVIRONMENTAL
UNKNOWN
```

The final CI policy combines these axes; it does not erase them.

---

# 118. PR quality verdict policy

A suggested deterministic priority order:

```text
1. Oracle contradiction on active high/critical obligation
2. Lost protection for active high/critical obligation
3. New blocking architecture/API contract violation
4. Required high-risk obligation unproven
5. Returned blocking debt
6. New high-confidence blocking debt
7. New/unknown flake in mandatory protection
8. AI budget exhausted on mandatory unresolved case
9. warning-only quality drift
10. proven/preserved
```

Possible overall states:

```text
PASS
PASS_WITH_WARNINGS
BLOCKED
NEEDS_HUMAN
NOT_ENOUGH_EVIDENCE
```

Never transform `NOT_ENOUGH_EVIDENCE` into PASS.

---

# 119. Additional high-value code-quality checks to add after P0

The earlier conversations emphasized squeezing as much as possible from Weavatrix. Beyond the current catalogue, candidates include:

### Public-surface blast-radius growth

A public function/route becomes transitively depended on by many more consumers.

### Cross-layer shortcut trend

One warning is noise; repeated new shortcut edges over several PRs show architectural erosion.

### Module cohesion drift

A module/community accumulates unrelated responsibilities or splits semantically while remaining one physical module.

### Dependency concentration

A single internal utility becomes a new critical shared dependency across unrelated features.

### Change-coupling anomaly

A file that historically changes with a stable partner now changes independently *and* runtime behavior changes unexpectedly.

### Test/prod coupling increase

Production implementation begins importing test-only helper/config or tests require increasingly implementation-specific internals.

### Public contract without negative-case protection

Endpoint exists and happy path is protected but historical/spec evidence expects error/permission cases that are absent.

### Feature-flag residue

Removed behavior leaves flag checks/config paths/dead branches.

### Error-path deprotection

Success coverage is preserved while specific previously covered error branch disappears.

### Retry/idempotency protection loss

Especially important for Node/Bun/Go services.

### Concurrency protection loss

Go race/context/cancellation paths or JS async ordering paths were historically protected and become unmeasured.

### Observability regression

Changed critical flow drops expected logs/metrics/tracing evidence when explicit observability contracts exist.

These should be added only when their evidence model is explicit and false-positive rate is measurable.

---

# 120. Component/story/a11y/network roadmap

After the core Spec→Protection→Proof loop:

## Wave A

```text
Storybook/Vitest adapter
semantic target model
structured a11y snapshots
base/head DOM/a11y delta
```

## Wave B

```text
network record/replay
MSW/project mock integration
fault profiles
HAR normalization
```

## Wave C

```text
visual baseline governance
native region diff
Figma design adapter
```

## Wave D

```text
clean-room/standards-based a11y rules
advanced component story generation
cross-browser policy matrices
```

The order is chosen to maximize deterministic value before adding expensive visual inference.

---

# 121. Updated phased build order after the full conversation history

The final recommended order is:

## P0 / Foundation

1. `wvq-domain`
2. OpenSpec compatibility
3. `quality.yaml`
4. OracleSeal
5. Spec Recovery evidence model
6. mandatory QA verification state machine
7. embedded `weavatrix-rust`
8. dual-revision impacted surface
9. flow lineage
10. test lineage
11. Base/Head ProtectionSnapshot
12. ProtectionDelta
13. Quality Debt Ratchet
14. result/coverage normalization
15. flow-aware minimal selection
16. Evidence Ledger/CAS
17. Proof/verdict
18. CLI
19. compact MCP
20. shadow benchmark

This is the smallest set that expresses the final thesis truthfully.

## P1 / Massive human-time reduction

21. Playwright TestProgram bridge
22. semantic browser observations
23. recorder
24. BehaviorGraph
25. base/head behavior differential
26. deterministic bug report
27. automatic retest
28. Flake Lab
29. safe healing
30. exception-first Quality Studio
31. BranchPilot quality projection

## P2 / Proof strength + autonomous gap closure

32. targeted mutation
33. metamorphic relations
34. network record/replay
35. fault profiles
36. cheap state/coverage-guided explorer
37. AI Budget Firewall enforcement
38. API/contract generators

## P3 / Frontend depth

39. Storybook/Vitest component adapter
40. advanced a11y
41. visual baseline governance
42. Figma structured design oracle
43. broader browser/device matrices by risk

## P4 / Optional ecosystem expansion

44. SightLoom long-session intelligence
45. Weavatrix Loom executor registry
46. broader cross-repository coordinated verification
47. MCP-protocol chaos/fuzz adapter
48. specialized performance/load profiles

---

# 122. P0 acceptance criteria

P0 is not complete until all are true:

```text
[ ] OpenSpec delta parsed with exact provenance
[ ] brownfield candidate spec cannot seal without QA
[ ] OracleSeal prevents test-oracle weakening
[ ] base and head graphs both participate in impact
[ ] removed base-only flow remains visible
[ ] old protecting test can be tracked after rename/refactor
[ ] global coverage increase cannot hide critical protection loss
[ ] new/existing/fixed/returned debt separated
[ ] dead code/clone/architecture/size/topology/API/coverage/history checks have provenance
[ ] minimal selection includes base historical protectors
[ ] test results and dynamic coverage normalized
[ ] revision-bound Proof stored immutably
[ ] quality_verify returns protection continuity
[ ] normal green run uses zero runtime LLM tokens
[ ] MCP cannot execute arbitrary shell
[ ] large artifacts never flood model context
[ ] benchmark compares full vs selected execution
```

---

# 123. P1 acceptance criteria

```text
[ ] manual UI session becomes semantic BehaviorTrace
[ ] session reports new coverage contribution
[ ] useful path can be promoted into reusable TestProgram
[ ] same program can replay with zero LLM tokens
[ ] base/head behavior diff prioritizes structured evidence before pixels
[ ] a known regression can produce reproduction/evidence automatically
[ ] fix retest is selected automatically from failed Proof
[ ] same test source but lost dynamic protection is detected
[ ] known flake classified without LLM
[ ] safe healing cannot alter sealed expectation
[ ] Quality Studio default view contains only actionable exceptions
```

---

# 124. Token-economics acceptance criteria

WVQ should publish a per-change cost report.

Example:

```text
AI COST
  spec planning            3,120 tokens
  recovery review packet       0 tokens
  execution                    0 tokens
  browser replay               0 tokens
  known triage                 0 tokens
  browser escape               0 calls
  vision                       0 calls

DEVELOPMENT AGENT REFERENCE
  42,500 tokens

QA / DEV
  7.3%
```

Failures:

```text
hidden unmetered model call
unbounded browser-agent loop
full screenshot sent repeatedly
whole repository inserted into test prompt
tool schema explosion
```

should be treated as product bugs.

---

# 125. Data-retention / privacy principles

Because the Evidence Ledger can contain sensitive UI/network data:

```text
redact secrets before CAS admission
never place raw auth tokens in MCP response
allow artifact retention policy
allow per-artifact privacy classification
support deterministic redaction profiles
separate metadata retention from blob retention
hash/provenance must remain meaningful after approved redaction form
```

Recorded browser sessions can include:

```text
cookies
headers
form contents
screenshots
responses
PII
```

so record/replay must have explicit secret/PII policies before enterprise use.

FerroSift or another deterministic transform layer may assist with redaction, but WVQ owns evidence-policy enforcement.

---

# 126. Security boundary for agent-driven quality

Agents may request semantic operations.

Allowed:

```text
run approved executor
run approved TestProgram
select impacted tests
request bounded evidence
request replay
request mutation profile approved by policy
```

Forbidden by default:

```text
arbitrary shell
arbitrary JavaScript eval
arbitrary filesystem writes
unapproved URL exfiltration
silent baseline approval
silent spec modification
silent OracleSeal change
unbounded browser navigation to arbitrary origins
```

Side-effecting operations must declare capability and policy.

---

# 127. Research sources to keep pinned in the repository

At minimum, preserve links and retrieval date in `docs/research.md`.

## Core specification / agent execution

- OpenSpec repository and docs: `https://github.com/Fission-AI/OpenSpec`
- OpenSpec OPSX/custom schemas: `https://github.com/Fission-AI/OpenSpec/blob/main/docs/opsx.md`
- OpenSpec writing specs: `https://github.com/Fission-AI/OpenSpec/blob/main/docs/writing-specs.md`
- Playwright: `https://github.com/microsoft/playwright`
- Playwright MCP: `https://github.com/microsoft/playwright-mcp`
- Playwright CLI: `https://github.com/microsoft/playwright-cli`

## Frontend testing ecosystem

- Storybook testing: `https://storybook.js.org/docs/writing-tests`
- Storybook Vitest addon: `https://storybook.js.org/docs/writing-tests/integrations/vitest-addon`
- Testing Library query principles: `https://testing-library.com/docs/queries/about/`
- Mock Service Worker: `https://mswjs.io/`

## Rust testing ecosystem

- cargo-nextest: `https://nexte.st/`
- cargo-llvm-cov: `https://github.com/taiki-e/cargo-llvm-cov`
- wiremock-rs: `https://github.com/LukeMathWalker/wiremock-rs`
- Goose: `https://book.goose.rs/`

## Research themes

- Plan-then-execute web agents: `https://arxiv.org/abs/2605.14290`
- ActionEngine / programmatic GUI memory: `https://arxiv.org/abs/2602.20502`
- KTester / testing knowledge: ICSE 2026 research track
- Panta / hybrid program analysis: ICSE 2026 research track
- Uber AutoCover: ICSE-SEIP 2026
- coding-before-testing oracle bias: `https://arxiv.org/abs/2607.05139`
- SWE-Mutation: `https://arxiv.org/abs/2605.22175`
- FlakyGuard: `https://arxiv.org/abs/2511.14002`
- LLM-generated test flakiness: `https://arxiv.org/abs/2601.08998`

At implementation time, pin exact tool versions and record licenses again rather than relying on this research snapshot.

---

# 128. What the product should eventually say on a real PR

The ideal output is not:

```text
483 tests passed
coverage 84%
```

It is:

```text
CHANGE
  add-sankey-others

INTENT
  8 active requirements
  15 scenarios
  1 recovered criterion awaiting QA

FLOW IMPACT
  affected flows          12
  preserved                5
  rewired                  4
  new                      2
  removed                  1

PROTECTION CONTINUITY
  preserved                7
  improved                 3
  degraded                 1
  lost                     0
  new unprotected          1

QUALITY DEBT
  new architecture         0
  new dead code            0
  new clone family         1 WARN
  oversized-growth         1 WARN
  fixed debt               2

RUNTIME
  selected tests          18 / 642
  component tests          6
  API tests                4
  E2E                      8
  full browser agent       0

PROOF
  PROVEN                  14
  PARTIAL                  0
  UNPROVEN                 1
  HUMAN_REQUIRED           1

AI
  execution tokens         0
  planning tokens       2,140
  vision calls             0

ACTION REQUIRED
  R18-S3:
  behavior during live refresh is not declared.

  [Accept candidate]
  [Mark bug]
  [Ask product]

VERDICT
  NEEDS_HUMAN
```

This is the product.

---

# 129. Final no-go rules

Future agents implementing this plan must not casually reopen these decisions.

Do not:

```text
replace Playwright
replace Vitest/Jest/Go test
build a giant browser MCP
make LLM clicks the default execution path
trust implementation-generated expected results
use only the head graph for impact
use global coverage as safety verdict
silently accept new baselines
silently repair business assertions
flatten evidence into one confidence score
make Cortex a hard dependency
make Weavatrix Loom a v1 dependency
build a second Weavatrix code graph
send full screenshots to models by default
send whole repo context for routine QA
run whole-repo mutation on every PR
create a full Cartesian role/browser/data matrix
force teams to clean all legacy debt before adoption
```

Any proposal to violate one of these needs benchmark/evidence showing why the existing decision is no longer appropriate.

---

# 130. Canonical architecture invariant

This is the final architecture distilled from all discussions:

> **AI is the compiler, spec assistant and exception handler. It is not the normal execution runtime.**

> **Weavatrix Quality protects continuity, not just after-state coverage.**

> **Implementation can suggest intent, but only declared/reviewed intent may become an OracleSeal.**

> **A passing test is not the product result; a revision-bound Proof is.**

> **Old debt does not block adoption, but new debt cannot hide inside legacy.**

> **A small QA team should spend its time on ambiguity and novel product risk, not on repeating known behavior.**

The complete canonical flow:

```text
                      PRODUCT INTENT
                           │
                           ▼
                        OpenSpec
                           │
          ┌────────────────┴────────────────┐
          │                                 │
   declared change                   no trustworthy spec?
          │                                 │
          │                                 ▼
          │                            Spec Recovery
          │                                 │
          │                         Mandatory QA Review
          │                                 │
          └────────────────┬────────────────┘
                           ▼
                     QualityContract
                           │
                           ▼
                       OracleSeal
                           │
                           ▼
                    TestObligations
                           │
                           ▼
             ┌────────────────────────┐
             │                        │
             ▼                        ▼
    Weavatrix base graph      Weavatrix head graph
             │                        │
             └───────────┬────────────┘
                         ▼
                 DualRevisionImpact
                         │
                         ▼
                     FlowLineage
                         │
             ┌───────────┴───────────┐
             ▼                       ▼
  BaseProtectionSnapshot   HeadProtectionSnapshot
             │                       │
             └───────────┬───────────┘
                         ▼
                  ProtectionDelta
                         │
                         ├── lost old protection?
                         ├── new unprotected flow?
                         ├── test oracle weakened?
                         └── protection improved?
                         │
                         ▼
               Quality Debt Ratchet
                         │
                         ├── dead code
                         ├── duplicates
                         ├── architecture
                         ├── cycles/imports
                         ├── file/function growth
                         ├── topology/blast radius
                         ├── API/transport drift
                         ├── coverage
                         └── history/hotspot risk
                         │
                         ▼
               Minimal Verification Plan
                         │
                         ▼
                     TestProgram
                         │
       ┌─────────────────┼───────────────────┐
       ▼                 ▼                   ▼
 Storybook/Vitest     Playwright         Node/Bun/Go
 components           flows/E2E           unit/API
       │                 │                   │
       └─────────────────┼───────────────────┘
                         ▼
                    Observations
                         │
                         ▼
                    BehaviorGraph
                         │
                         ▼
                    BehaviorDelta
                         │
                         ▼
         SpecDelta + CodeDelta + BehaviorDelta
                         │
                         ▼
                    DeltaTriangle
                         │
                         ▼
                Evidence Ledger / CAS
                         │
                         ▼
                        Proof
                         │
             ┌───────────┼────────────┐
             ▼           ▼            ▼
          Mutation      Flake      Metamorphic
             │           │            │
             └───────────┼────────────┘
                         ▼
                   QualityVerdict
                         │
             ┌───────────┴─────────────┐
             ▼                         ▼
       auto proven                 unresolved only
             │                         │
             ▼                         ▼
           CI/PR                 Quality Analyst
                                       │
                                       ▼
                              explicit human decision
                                       │
                                       ▼
                            future permanent protection
```

**This is the canonical `weavatrix-quality` product plan.**
