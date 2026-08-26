export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue }
export type RunScope = 'impacted' | 'all'
export type EvidencePolicy = 'standard' | 'minimal' | 'none'
export type ContextPurpose = 'spec' | 'implementation' | 'review'
export type ModelKind = 'planning' | 'runtime' | 'browser_escape' | 'vision'
export type McpProfile = 'default' | 'recovery' | 'protection' | 'authoring'

export interface CallOptions { signal?: AbortSignal }
export interface RangeOptions extends CallOptions { change?: string; base?: string; head?: string }

export interface ContextReply {
    change: string
    purpose: string
    requirements: string[]
    obligations: string[]
    heuristics: string[]
    coverage: string[]
    truncated: boolean
    tokens_used: number
    token_budget: number
}

export interface PlanReply {
    change: string
    requirements: string[]
    obligations: string[]
    risk: string[]
    existing_proofs: string[]
    gaps: string[]
    checks: string[]
    executed: false
}

export interface RunReply {
    run_id: string
    change: string
    base: string
    head: string
    requested_scope: RunScope
    scope: RunScope
    scope_reason: string
    status: string
    executed: boolean
    outcome: 'passed' | 'failed' | 'error'
    selected_test_count: number
    available_test_count: number
    executor_invocations: number
    browser_programs: number
    behavior_state_count: number
    new_behavior_state_count: number
    behavior_edge_count: number
    new_behavior_edge_count: number
    recorded_test_count: number
    failed_test_count: number
    flaky_test_count: number
    unknown_failure_count: number
    artifact_handles: string[]
}

export interface StatusReply {
    run_id: string | null
    status: string
    outcome: string | null
    handles: string[]
}

export interface ProofSummary {
    id: string
    requirement: string
    obligation: string
    verdict: string
}

/** Composed change-level state. Never a single quality percentage. */
export type ChangeVerdictState =
    | 'BLOCKED'
    | 'NEEDS_HUMAN'
    | 'NOT_ENOUGH_EVIDENCE'
    | 'PASS_WITH_WARNINGS'
    | 'PASS'

/**
 * State of one axis. `not_applicable` means the change has no surface the axis
 * can measure; `unmeasured` means it does and no evidence arrived. Neither is
 * ever reported as clean.
 */
export type AxisState = 'not_applicable' | 'clean' | 'warnings' | 'blocking' | 'unmeasured'

export type Severity = 'info' | 'warn' | 'error'

export interface BlockingReason {
    rank: number
    code: string
    axis: string
    subject: string
    detail: string
}

export interface Limitation {
    axis: string
    detail: string
}

export interface ProofAxis {
    state: AxisState
    proven: number
    partial: number
    unproven: number
    contradicted: number
    human_required: number
    contradicted_obligations: string[]
    unproven_mandatory: string[]
    ambiguous_obligations: string[]
}

export interface ProtectionSummary {
    preserved: number
    improved: number
    degraded: number
    lost: number
    replaced: number
    relocated: number
    new_unprotected: number
}

export interface ProtectionFinding {
    check: string
    severity: Severity
    subject: string
    detail: string
}

export interface ProtectionAxis {
    state: AxisState
    measured: boolean
    summary: ProtectionSummary
    lost_flows: string[]
    lost_critical_branches: string[]
    blocking_findings: ProtectionFinding[]
    warning_findings: ProtectionFinding[]
}

export interface DebtItem {
    id: string
    rule: string
    blocking: boolean
}

export interface DebtAxis {
    state: AxisState
    comparison_present: boolean
    existing: number
    fixed: number
    excepted: number
    new: DebtItem[]
    returned: DebtItem[]
}

export interface StabilityAxis {
    state: AxisState
    measured: boolean
    flaky: number
    unknown_failures: number
    unresolved_mandatory_flakes: string[]
}

export interface AiAxis {
    state: AxisState
    runtime_tokens: number
    budget_exhausted: boolean
    unresolved_decisions: string[]
}

export interface UiFindingRef {
    check: string
    severity: Severity
    subject: string
    route: string
    viewport: string
    detail: string
}

export interface UiIntegrityAxis {
    state: AxisState
    new: UiFindingRef[]
    returned: UiFindingRef[]
    existing: number
    fixed: number
    excepted: number
    unmeasured_states: string[]
    truncated: boolean
}

export interface ChangeQualityVerdict {
    state: ChangeVerdictState
    proof: ProofAxis
    protection: ProtectionAxis
    debt: DebtAxis
    stability: StabilityAxis
    ai: AiAxis
    ui_integrity: UiIntegrityAxis
    blocking_reasons: BlockingReason[]
    limitations: Limitation[]
}

export interface ApplicationSurfaceView {
    /** False when the run never stored a surface graph. Missing is not empty. */
    present: boolean
    truncated: boolean
    protected: string[]
    partial: string[]
    unmeasured: string[]
}

export interface VerifyReply {
    change: string
    /** Combined ProofVerdict token, kept for backward compatibility. */
    verdict: string
    /** Driven by the composite verdict, not by `verdict` alone. */
    blocking: boolean
    proofs: ProofSummary[]
    state: ChangeVerdictState
    quality: ChangeQualityVerdict
    /** Read-only surface projection. Never a gate. */
    application_surface: ApplicationSurfaceView
    /** Read-only evidence matrix. Never a gate. */
    surface_evidence: SurfaceEvidenceMatrixView
    /** Read-only cheapest-evidence plan. Never a gate. */
    evidence_plan: CheapestEvidencePlanView
    /** Stage A: facts unchanged, process exit stays 0. */
    observe_only: boolean
}

export interface SurfaceEvidenceRow {
    surface: string
    kind: string
    intent: 'present' | 'absent' | 'unmeasured'
    runtime: 'present' | 'absent' | 'unmeasured'
    test: 'present' | 'absent' | 'unmeasured'
    proof: 'present' | 'absent' | 'unmeasured'
    protection: 'present' | 'absent' | 'unmeasured'
    ui: 'present' | 'absent' | 'unmeasured'
    a11y: 'present' | 'absent' | 'unmeasured'
    mutation: 'present' | 'absent' | 'unmeasured'
}

export interface SurfaceEvidenceMatrixView {
    present: boolean
    truncated: boolean
    surfaces: SurfaceEvidenceRow[]
}

export interface EvidencePlan {
    surface: string
    kind: string
    column:
        | 'intent'
        | 'runtime'
        | 'test'
        | 'proof'
        | 'protection'
        | 'ui'
        | 'a11y'
        | 'mutation'
    cheapest:
        | 'existing_test_adaptation'
        | 'recorded_session'
        | 'storybook_flow'
        | 'browser_explore'
        | 'ai_test_program'
        | null
    producers: Array<{
        producer:
            | 'existing_test_adaptation'
            | 'recorded_session'
            | 'storybook_flow'
            | 'browser_explore'
            | 'ai_test_program'
        cost: number
    }>
}

export interface CheapestEvidencePlanView {
    present: boolean
    truncated: boolean
    gaps: EvidencePlan[]
}

export interface ExplainReply {
    id: string
    kind: string
    summary: string
    provenance: string[]
}

export interface SpecValidateReply {
    change: string
    requirements: number
    obligations: number
    ok: true
}

export interface SpecSealReply {
    change: string
    seal_id: string
    digest: string
    obligations: number
}

export interface DebtReply {
    revision: string | null
    base: string
    head: string
    comparison_present: boolean
    existing: number
    new: number
    fixed: number
    returned: number
    excepted: number
    findings: string[]
    limitations: string[]
}

export interface SelectReply {
    revision: string | null
    base: string
    head: string
    algorithm: string
    selected: string[]
    uncovered_mandatory: string[]
    explanations: string[][]
    executed: false
    selection_complete: boolean
}

export interface ModelReply {
    change: string
    kind: ModelKind
    model: string
    text: string
    input_tokens: number
    output_tokens: number
    cost_micros: number
}

export interface AuthoringObligation {
    id: string
    requirement: string
    scenario: string
    kind: string
    risk: string
    condition: JsonValue | null
    expected: JsonValue | null
    required_evidence: string[]
}

export interface AuthorDraftReply {
    change: string
    revision: string
    base: string
    head: string
    changed_files: string[]
    context: string[]
    obligations: AuthoringObligation[]
    truncated: boolean
    tokens_used: number
    token_budget: number
    candidate: JsonValue | null
    model_usage: null | {
        model: string
        input_tokens: number
        output_tokens: number
        cost_micros: number
    }
}

export interface AuthorValidateReply {
    change: string
    seal_id: string
    program_id: string
    program: JsonValue
    obligations: string[]
    valid: true
    persisted: false
}

export interface AuthorPreviewReply {
    preview_id: string
    change: string
    revision: string
    program_id: string
    passed: boolean
    asserted: string[]
    contradicted: string[]
    failure: string | null
    observation_handles: string[]
    screenshot_handles: string[]
    trace_handle: string | null
    program_persisted: false
}

export interface RecordReply {
    session_id: string
    change: string
    revision: string
    captured_events: number
    useful: boolean
    discarded: boolean
    discard_reason: string | null
    new_behavior_states: number
    new_behavior_edges: number
    linked_obligations: string[]
    new_obligations: string[]
    api_operations: string[]
    new_api_operations: string[]
    limitations: string[]
    candidate: JsonValue | null
    preview: AuthorPreviewReply | null
    trace_handle: string | null
    network_profile_handle: string | null
    runtime_llm_tokens: 0
}

export interface IngestJournalReply {
    session_id: string
    change: string
    revision: string
    captured_events: number
    useful: boolean
    discarded: boolean
    discard_reason: string | null
    new_behavior_states: number
    new_behavior_edges: number
    observed_only: true
    seal_eligible: false
    trace_handle: string | null
    journal_handle: string | null
    runtime_llm_tokens: 0
}

export interface IngestCassetteReply {
    origin: string
    revision: string
    captured_entries: number
    omitted: number
    useful: boolean
    discarded: boolean
    discard_reason: string | null
    limitations: string[]
    replay_enabled: false
    seal_eligible: false
    profile_handle: string | null
    runtime_llm_tokens: 0
}

export interface BaselineReply {
    change: string
    revision: string
    fingerprints: string[]
    recorded: number
    new_unbaselined: number
    observed_only: true
    seal_eligible: false
    runtime_llm_tokens: 0
}

export interface AuthorPromoteReply {
    change: string
    revision: string
    seal_id: string
    program_id: string
    program_revision: number
    persisted: true
    created: boolean
}

export type AuthorHealEdit =
    | { edit: 'retarget'; step: number; target: Record<string, JsonValue> }
    | { edit: 'insert_wait'; after: number; condition: Record<string, JsonValue> }

export interface AuthorHealReply {
    preview_id: string
    change: string
    revision: string
    seal_id: string
    program_id: string
    previous_program_revision: number
    program_revision: number | null
    passed: boolean
    asserted: string[]
    contradicted: string[]
    failure: string | null
    observation_handles: string[]
    screenshot_handles: string[]
    trace_handle: string | null
    persisted: boolean
    created: boolean
}

export interface WvqClientOptions {
    repo?: string
    binary?: string
    timeoutMs?: number
    invoke?: (binary: string, args: string[], options?: { signal?: AbortSignal; timeoutMs?: number }) => Promise<unknown>
}

export class WvqClient {
    constructor(options?: WvqClientOptions)
    specValidate(options?: { change?: string; signal?: AbortSignal }): Promise<SpecValidateReply>
    specSeal(options?: { change?: string; signal?: AbortSignal }): Promise<SpecSealReply>
    analyze(options?: { change?: string; purpose?: ContextPurpose; tokenBudget?: number; signal?: AbortSignal }): Promise<ContextReply>
    debt(options?: RangeOptions): Promise<DebtReply>
    select(options?: RangeOptions): Promise<SelectReply>
    run(options?: RangeOptions & { scope?: RunScope; evidencePolicy?: EvidencePolicy }): Promise<RunReply>
    record(options?: RangeOptions & {
        route?: string
        fixtureValues?: Record<string, string>
        idleTimeoutMs?: number
        maxEvents?: number
        headless?: boolean
    }): Promise<RecordReply>
    ingestJournal(options?: RangeOptions & { file?: string; journal?: string }): Promise<IngestJournalReply>
    ingestCassette(options: { origin: string; file?: string; har?: string; signal?: AbortSignal }): Promise<IngestCassetteReply>
    baseline(options?: RangeOptions): Promise<BaselineReply>
    status(options?: CallOptions): Promise<StatusReply>
    verify(options?: { change?: string; observeOnly?: boolean; signal?: AbortSignal }): Promise<VerifyReply>
    explain(id: string, options?: CallOptions): Promise<ExplainReply>
    plan(options?: { change?: string; signal?: AbortSignal }): Promise<PlanReply>
    model(options: { change?: string; kind: ModelKind; prompt: string; signal?: AbortSignal }): Promise<ModelReply>
}

export interface WvqMcpClientOptions {
    repo?: string
    profile?: McpProfile
    change?: string
    base?: string
    head?: string
    binary?: string
    timeoutMs?: number
    invoke?: (
        binary: string,
        args: string[],
        tool: string,
        input: Record<string, JsonValue>,
        options?: { signal?: AbortSignal; timeoutMs?: number },
    ) => Promise<unknown>
}

export class WvqMcpClient {
    constructor(options?: WvqMcpClientOptions)
    call<T = JsonValue>(tool: string, input?: Record<string, JsonValue>, options?: { signal?: AbortSignal; timeoutMs?: number }): Promise<T>
    draft(options?: { tokenBudget?: number; useModel?: boolean; signal?: AbortSignal }): Promise<AuthorDraftReply>
    validate(program: Record<string, JsonValue>, options?: CallOptions): Promise<AuthorValidateReply>
    preview(
        program: Record<string, JsonValue>,
        options?: { screenshot?: boolean; trace?: boolean; signal?: AbortSignal; timeoutMs?: number },
    ): Promise<AuthorPreviewReply>
    promote(
        previewId: string,
        program: Record<string, JsonValue>,
        options?: CallOptions,
    ): Promise<AuthorPromoteReply>
    record(options?: {
        route?: string
        fixtureValues?: Record<string, string>
        idleTimeoutMs?: number
        maxEvents?: number
        headless?: boolean
        signal?: AbortSignal
        timeoutMs?: number
    }): Promise<RecordReply>
    heal(
        programId: string,
        expectedProgramRevision: number,
        edits: AuthorHealEdit[],
        options?: { screenshot?: boolean; trace?: boolean; signal?: AbortSignal; timeoutMs?: number },
    ): Promise<AuthorHealReply>
}

export function resolveBinary(kind?: 'wvq' | 'mcp' | 'bench'): string
