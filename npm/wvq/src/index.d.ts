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

export interface VerifyReply {
    change: string
    verdict: string
    blocking: boolean
    proofs: ProofSummary[]
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
    status(options?: CallOptions): Promise<StatusReply>
    verify(options?: { change?: string; signal?: AbortSignal }): Promise<VerifyReply>
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
    heal(
        programId: string,
        expectedProgramRevision: number,
        edits: AuthorHealEdit[],
        options?: { screenshot?: boolean; trace?: boolean; signal?: AbortSignal; timeoutMs?: number },
    ): Promise<AuthorHealReply>
}

export function resolveBinary(kind?: 'wvq' | 'mcp' | 'bench'): string
