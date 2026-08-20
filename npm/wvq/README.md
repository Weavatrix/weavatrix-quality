# wvq

Native Weavatrix Quality binaries plus a typed JavaScript boundary. Rust remains the only implementation of policy, test selection, evidence, proof, budgets, and MCP semantics.

```sh
npx wvq --repo . plan --change current
npx wvq mcp --repo .
npx wvq bench --repo . --change current --base origin/main --head WORKTREE
```

```js
import { WvqClient } from 'wvq'
import { WvqMcpClient } from 'wvq/mcp'

const quality = new WvqClient({ repo: process.cwd() })
const run = await quality.run({
    change: 'current',
    base: 'origin/main',
    head: 'WORKTREE',
    scope: 'impacted',
    evidencePolicy: 'minimal',
})

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
const healed = await authoring.heal(promoted.program_id, promoted.program_revision, [
    { edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' } },
])
```

The package carries native `wvq`, `wvq-mcp`, and `wvq-bench` programs for Windows, macOS, and Linux on x64 and arm64. `WVQ_BINARY`, `WVQ_MCP_BINARY`, and `WVQ_BENCH_BINARY` can select an explicitly installed matching binary.

The npm launchers never use a shell. JavaScript is a typed process boundary; policy, selection, evidence, proof, budgets, and MCP schemas still execute in Rust. Authoring exposes only `draft`, `validate`, `preview`, explicit passing-preview `promote`, and locator/wait-only `heal`; it does not add browser click/eval or arbitrary command tools.

Browser preview uses Playwright from the repository's configured `browser.module_root`. Install the Playwright package and the engines you intend to run in that repository, for example:

```sh
npm install --save-dev playwright
npx playwright install chromium
```
