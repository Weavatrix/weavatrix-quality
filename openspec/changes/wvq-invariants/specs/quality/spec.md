# Delta for Weavatrix Quality invariants

## Purpose

Product intent for WVQ itself. These requirements name fail-closed rules that
already exist in the implementation. They do not invent tests, bindings, or a
seal.

## ADDED Requirements

### Requirement: Unmeasured Never Clean
Missing evidence SHALL never be treated as evidence of absence. An axis that
was in scope and was not measured SHALL be `unmeasured`, never clean.

#### Scenario: Missing evidence is not absence
- GIVEN a required axis with a measurable surface and no stored evidence
- WHEN the composite change verdict is assembled
- THEN the axis is `unmeasured`
- AND the verdict is not a pass on that axis

### Requirement: Observed Only Never Seals
`OBSERVED_ONLY` evidence SHALL remain baseline or journal evidence. It SHALL
not become a normative `OracleSeal`.

#### Scenario: Observed baseline cannot become a seal
- GIVEN an observed-only debt snapshot or continuous observation journal
- WHEN proof or sealing is requested from that evidence alone
- THEN the evidence stays `OBSERVED_ONLY`
- AND no oracle seal is created from it

### Requirement: Test Node Cannot Satisfy Production Code Delta
A test-file binding SHALL not satisfy a production `CodeDelta`. Only
implementation Weavatrix nodes on an `ObligationCodeSurface` SHALL make the
code axis true.

#### Scenario: Test binding is not a production surface
- GIVEN a program bound to a test path and a production source change
- WHEN `scoped_code_delta` is computed
- THEN the test node does not make `code_changed` true
- AND production ownership requires directed Weavatrix reach

### Requirement: Revision Drift Invalidates Proof
A proof SHALL be bound to an exact repository revision. Drifted or ambiguous
revision identity SHALL fail closed rather than reuse the proof.

#### Scenario: Drifted revision is not the same proof
- GIVEN a stored proof for one revision identity
- WHEN the checked-out revision no longer matches that identity
- THEN the proof is not accepted as current
- AND the path fails closed

### Requirement: Raw Secret Never Enters Evidence
Evidence artifacts SHALL persist no raw secrets. Request bodies, cookies,
bearer tokens, JWT-like strings, and configured sensitive keys SHALL not enter
the journal, replay profile, or comparison token.

#### Scenario: Evidence ledger never stores a raw secret
- GIVEN a captured browser or network observation
- WHEN the observation is persisted
- THEN the stored record contains no raw secret
- AND identity uses method, path, content type, and a body digest

### Requirement: Lost Critical Protection Blocks
A lost critical protection delta SHALL block the composite change verdict. A
green suite or a global coverage gain SHALL not hide that local loss.

#### Scenario: Lost critical protection is a blocking verdict
- GIVEN a base protection snapshot that covered a critical flow
- WHEN head no longer protects that flow
- THEN `quality_verify` is `BLOCKED`
- AND a `PROVEN` behavioural proof cannot suppress the loss

### Requirement: Green Path Spends Zero Model Tokens
Normal green-path verification SHALL spend zero runtime LLM tokens. A model
call SHALL not be required to assemble proof, protection, or the composite
verdict.

#### Scenario: Normal verification spends no model tokens
- GIVEN a change that can be verified from repository evidence
- WHEN `wvq run` and `wvq verify` execute the ordinary path
- THEN runtime model tokens remain 0
- AND no vision or browser-escape model call is made

### Requirement: Unowned Mutant Cannot Strengthen Proof
A mutant without an obligation owner SHALL stay `unmeasured`. It SHALL not
produce `killed` and SHALL not strengthen Proof for any obligation.

#### Scenario: Unowned mutant stays unmeasured
- GIVEN a source mutant whose owners cannot be resolved
- WHEN mutation results are attributed
- THEN the mutant is `unmeasured`
- AND no obligation proof is strengthened by it

### Requirement: Aggregate Coverage Cannot Claim Exact Case
Aggregate coverage from a multi-test batch SHALL stay executor-level. It SHALL
not be guessed onto each member as exact case-level proof.

#### Scenario: Aggregate coverage stays executor-level
- GIVEN a successful executor invocation that ran more than one test
- WHEN coverage is attributed to tests
- THEN the batch remains aggregate
- AND no member claims an exact case identity from that coverage
