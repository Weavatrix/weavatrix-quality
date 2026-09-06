# `@weavatrix/wvq`

Native **Weavatrix Quality** binaries (`wvq`, `wvq-mcp`, `wvq-bench`) for
Windows / macOS / Linux (x64 + arm64), plus a typed JavaScript client.

Rust remains the only implementation of policy, selection, evidence, proof,
budgets, and MCP schemas. This package is a process boundary â€” never a second
engine.

**Alpha `0.1.0-alpha.3`.** [CHANGELOG](https://github.com/Weavatrix/weavatrix-quality/blob/main/CHANGELOG.md) Â·
[GitHub](https://github.com/Weavatrix/weavatrix-quality) Â·
[MCP Registry](https://registry.modelcontextprotocol.io/) (`io.github.Weavatrix/weavatrix-quality`)

---

## Install

```sh
npm install --save-dev @weavatrix/wvq@0.1.0-alpha.3

# or without adding a dependency
npx @weavatrix/wvq@0.1.0-alpha.3 --help
```

Unscoped `wvq` is reserved by npm similarity rules. Always use `@weavatrix/wvq`.

Bins on PATH after install:

| Bin | Role |
| --- | --- |
| `wvq` | CLI |
| `wvq-mcp` | MCP host (also: `wvq mcp â€¦`) |
| `wvq-bench` | Impacted-vs-full shadow bench |

Override native paths with `WVQ_BINARY`, `WVQ_MCP_BINARY`, `WVQ_BENCH_BINARY`
when you install matching binaries yourself.

---

## CLI examples

```sh
# discovery (read-only)
npx @weavatrix/wvq@0.1.0-alpha.3 doctor

# first-time policy
npx @weavatrix/wvq@0.1.0-alpha.3 init

# compile OpenSpec obligations
npx @weavatrix/wvq@0.1.0-alpha.3 spec validate --change current

# plan without executing
npx @weavatrix/wvq@0.1.0-alpha.3 plan --change current

# impacted run
npx @weavatrix/wvq@0.1.0-alpha.3 run \
  --change current \
  --base origin/main \
  --head HEAD \
  --scope impacted \
  --evidence-policy minimal

# composite verdict
npx @weavatrix/wvq@0.1.0-alpha.3 verify --change current

# Stage A CI (exit 0 even when UNPROVEN)
npx @weavatrix/wvq@0.1.0-alpha.3 verify --change current --observe-only true

# passive browser capture
npx @weavatrix/wvq@0.1.0-alpha.3 record --change current --route /dashboard

# shadow selected vs full
npx @weavatrix/wvq@0.1.0-alpha.3 bench \
  --repo . --change current --base origin/main --head WORKTREE
```

### Useful flags

```text
wvq --repo <path> <command> â€¦
wvq run --scope impacted|all --evidence-policy standard|minimal|none
wvq verify --observe-only true|false
wvq record --route /path --idle-ms 3000 --max-events 200
```

Exit `2` = blocking contradicted verify. Exit `1` = unresolved / ordinary failure.

---

## MCP examples

### Cursor

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "npx",
      "args": [
        "-y",
        "@weavatrix/wvq@0.1.0-alpha.3",
        "mcp",
        "--repo",
        "."
      ]
    }
  }
}
```

Prefer an **absolute** `--repo` path in real configs.

### Default profile â€” 7 tools

```text
quality_context  quality_plan  quality_run  quality_status
quality_verify   quality_explain  quality_evidence
```

Ask the agent:

> Call `quality_plan` for change `checkout-fix`, then `quality_run` with
> base `origin/main` and head `WORKTREE`, then `quality_verify`.

### Authoring profile

```sh
npx @weavatrix/wvq@0.1.0-alpha.3 mcp --repo . \
  --profile authoring \
  --change current \
  --base origin/main \
  --head WORKTREE
```

```text
quality_test_draft  quality_test_validate  quality_test_preview
quality_test_promote  quality_test_record  quality_test_heal
```

No arbitrary shell. Large artifacts return as handles.

---

## JavaScript API examples

```js
import { WvqClient } from '@weavatrix/wvq'
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const repo = process.cwd()
const change = 'current'

// â€”â€”â€” typed CLI boundary â€”â€”â€”
const wvq = new WvqClient({ repo })

await wvq.specValidate({ change })
const plan = await wvq.plan({ change })
const run = await wvq.run({
  change,
  base: 'origin/main',
  head: 'WORKTREE',
  scope: 'impacted',
  evidencePolicy: 'minimal',
})
const verify = await wvq.verify({ change })
console.log({
  runId: run.run_id,
  outcome: run.outcome,
  state: verify.state,
  proven: verify.quality.proof.proven,
  unproven: verify.quality.proof.unproven,
})

const debt = await wvq.debt({ change, base: 'origin/main', head: 'HEAD' })
const selected = await wvq.select({ change, base: 'origin/main', head: 'HEAD' })
const status = await wvq.status()

// cancel a long run
const ac = new AbortController()
setTimeout(() => ac.abort(), 120_000)
await wvq.run({ change, base: 'origin/main', head: 'HEAD', signal: ac.signal })

// â€”â€”â€” MCP one-shot tools â€”â€”â€”
const mcp = new WvqMcpClient({ repo, profile: 'default', change })
await mcp.call('quality_context', { change, purpose: 'implementation' })
await mcp.call('quality_verify', { change })

const authoring = new WvqMcpClient({
  repo,
  profile: 'authoring',
  change,
  base: 'origin/main',
  head: 'WORKTREE',
})
const draft = await authoring.draft({ useModel: false })
// agent fills candidateProgram from draft.obligations + draft.context
const validated = await authoring.validate(candidateProgram)
const preview = await authoring.preview(validated.program, {
  screenshot: true,
  trace: true,
})
if (preview.passed) {
  const promoted = await authoring.promote(preview.preview_id, validated.program)
  await authoring.heal(promoted.program_id, promoted.program_revision, [
    { edit: 'insert_wait', after: 0, condition: { kind: 'url', route: '/ready' } },
  ])
}
```

Types: `src/index.d.ts`. More samples:
[examples on GitHub](https://github.com/Weavatrix/weavatrix-quality/tree/main/examples).

---

## Browser / Playwright

Authoring preview and UI programs use Playwright from the **verified repo**
(`browser.module_root` in `.weavatrix-quality/config.yaml`):

```sh
npm install --save-dev playwright
npx playwright install chromium
```

---

## What this package does not do

- Does not reimplement Rust policy / proof in TypeScript
- Does not spawn a shell for argv
- Does not auto-heal sealed business expectations
- Does not publish an unscoped `wvq` name

## License

MIT
