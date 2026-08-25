import { WvqClient, type AxisState, type ChangeVerdictState, type Severity } from 'wvq'
import { WvqMcpClient } from 'wvq/mcp'

const client = new WvqClient({ repo: '.' })
const run = await client.run({
    change: 'current',
    base: 'origin/main',
    head: 'WORKTREE',
    scope: 'impacted',
    evidencePolicy: 'minimal',
})
run.scope_reason satisfies string

const authoring = new WvqMcpClient({
    repo: '.',
    profile: 'authoring',
    change: 'current',
    base: 'origin/main',
    head: 'WORKTREE',
})
const draft = await authoring.draft({ tokenBudget: 8_000 })
draft.obligations satisfies Array<{ id: string }>

// The composite verdict is part of the public surface, and each axis keeps its
// own facts rather than collapsing into a score.
const verified = await client.verify({ change: 'current' })
verified.state satisfies ChangeVerdictState
verified.verdict satisfies string
verified.blocking satisfies boolean
verified.quality.proof.state satisfies AxisState
verified.quality.protection.lost_critical_branches satisfies string[]
verified.quality.debt.new satisfies Array<{ id: string; rule: string; blocking: boolean }>
verified.quality.stability.unresolved_mandatory_flakes satisfies string[]
verified.quality.ai.runtime_tokens satisfies number
verified.quality.ui_integrity.new satisfies Array<{
    check: string
    severity: Severity
    subject: string
    route: string
    viewport: string
    detail: string
}>
verified.quality.ui_integrity.unmeasured_states satisfies string[]
verified.quality.blocking_reasons satisfies Array<{ rank: number; code: string; axis: string }>
verified.quality.limitations satisfies Array<{ axis: string; detail: string }>
verified.application_surface.present satisfies boolean
verified.application_surface.protected satisfies string[]
verified.application_surface.partial satisfies string[]
verified.application_surface.unmeasured satisfies string[]
verified.surface_evidence.present satisfies boolean
verified.surface_evidence.surfaces satisfies Array<{
    surface: string
    intent: 'present' | 'absent' | 'unmeasured'
    protection: 'present' | 'absent' | 'unmeasured'
}>
