import { WvqClient } from 'wvq'
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
