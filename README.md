# Weavatrix Quality (`wvq`)

[![CI](https://github.com/Weavatrix/weavatrix-quality/actions/workflows/ci.yml/badge.svg)](https://github.com/Weavatrix/weavatrix-quality/actions/workflows/ci.yml)
[![npm](https://img.shields.io/npm/v/@weavatrix/wvq/alpha.svg)](https://www.npmjs.com/package/@weavatrix/wvq)
[![MCP](https://img.shields.io/badge/MCP-io.github.Weavatrix%2Fweavatrix--quality-blue)](https://registry.modelcontextprotocol.io/)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

**Know what your change can break — and whether it is still protected.**

Weavatrix Quality (WVQ) turns OpenSpec intent + a Git change into revision-bound
proof using your existing runners. The ordinary green path spends **0 runtime LLM
tokens**. Rust owns policy, selection, evidence, and proof; npm/JS is a typed
boundary around the same binaries.

```text
OpenSpec   → what must remain true
Weavatrix  → what changed and what it can affect
Runners    → smallest safe protection set
WVQ        → same-revision Proof + composite verdict
```

| Surface | Package |
| --- | --- |
| **npm (primary)** | [`@weavatrix/wvq`](https://www.npmjs.com/package/@weavatrix/wvq) |
| **MCP Registry** | `io.github.Weavatrix/weavatrix-quality` |
| **crates.io** | [`wvq-cli`](https://crates.io/crates/wvq-cli) · [`wvq-mcp`](https://crates.io/crates/wvq-mcp) · [`wvq-bench`](https://crates.io/crates/wvq-bench) |
| **Release** | [v0.1.0-alpha.1](https://github.com/Weavatrix/weavatrix-quality/releases/tag/v0.1.0-alpha.1) |

**Alpha `0.1.0-alpha.1`.** Useful first product loop (one plan → one execution →
one report). Not the full v1 DoD — see [CHANGELOG](CHANGELOG.md) and
[ADR 0003](docs/adr/0003-alpha-orchestration-gate.md).

---

## Install

```sh
# primary distribution (ships wvq + wvq-mcp + wvq-bench for 6 platforms)
npm install --save-dev @weavatrix/wvq@0.1.0-alpha.1

# one-shot without adding a dependency
npx @weavatrix/wvq@0.1.0-alpha.1 --help

# Rust binaries (alpha crates; API unstable)
cargo install wvq-cli --version 0.1.0-alpha.1
cargo install wvq-mcp --version 0.1.0-alpha.1
cargo install wvq-bench --version 0.1.0-alpha.1
```

Unscoped `wvq` is blocked by npm name-similarity rules — always use `@weavatrix/wvq`.

---

## 5-minute quickstart

```sh
cd your-repo

# 1) read-only discovery (never writes, never seals)
npx @weavatrix/wvq@0.1.0-alpha.1 doctor

# 2) write fail-closed policy (first time only)
npx @weavatrix/wvq@0.1.0-alpha.1 init

# 3) compile OpenSpec obligations for a change folder
npx @weavatrix/wvq@0.1.0-alpha.1 spec validate --change current

# 4) run the smallest safe protection set
npx @weavatrix/wvq@0.1.0-alpha.1 run \
  --change current \
  --base origin/main \
  --head HEAD \
  --scope impacted \
  --evidence-policy minimal

# 5) read-only composite verdict from stored evidence
npx @weavatrix/wvq@0.1.0-alpha.1 verify --change current
```

Need bindings before proofs can seal? Start from [.weavatrix-quality/config.yaml](#repository-policy)
and the [examples](examples/).

---

## CLI cookbook

All commands take `--repo <path>` (default: `.`). Prefer absolute paths in CI.

### Day-to-day cycle

```sh
alias wvq='npx @weavatrix/wvq@0.1.0-alpha.1'

wvq doctor
wvq plan --change checkout-fix
wvq select --change checkout-fix --base origin/main --head WORKTREE
wvq run --change checkout-fix --base origin/main --head WORKTREE --scope impacted
wvq status
wvq verify --change checkout-fix
wvq explain <proof-or-finding-id>
```

### Spec / seal / debt

```sh
wvq spec validate --change checkout-fix
wvq spec seal --change checkout-fix          # fail-closed; needs valid obligations
wvq debt --change checkout-fix --base origin/main --head HEAD
wvq baseline --change checkout-fix --decision observed_only   # CLI-only, not MCP
```

### Observe-only CI (Stage A)

```sh
# facts stay honest (UNPROVEN / NOT_ENOUGH_EVIDENCE), process exit stays 0
wvq verify --change checkout-fix --observe-only true
```

### Browser record → promote later via MCP/Studio

```sh
wvq record --change checkout-fix --route /checkout
wvq ingest-journal --file .weavatrix-quality/journals/session.json
wvq ingest-cassette --file captures/checkout.har --origin https://app.example.test
```

### Shadow selected-vs-full (bench)

```sh
wvq bench --repo . --change checkout-fix --base origin/main --head WORKTREE
# or: npx @weavatrix/wvq@0.1.0-alpha.1 bench --repo . ...
```

### Exit codes

| Code | Meaning |
| --- | --- |
| `0` | success / observe-only (even when verdict is unresolved) |
| `1` | unresolved evidence / ordinary failure |
| `2` | blocking `CONTRADICTED` verify |

Full CLI surface: `wvq --help`. Deep product rules live in
[`docs/STATUS.md`](docs/STATUS.md).

---

## MCP cookbook

WVQ ships as an **mcport** MCP host. Default profile = **7 tools**, no shell,
large artifacts as handles only.

### Cursor (`mcp.json`)

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "npx",
      "args": [
        "-y",
        "@weavatrix/wvq@0.1.0-alpha.1",
        "mcp",
        "--repo",
        "C:/path/to/your-repo"
      ]
    }
  }
}
```

### Claude Desktop

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "npx",
      "args": ["-y", "@weavatrix/wvq@0.1.0-alpha.1", "mcp", "--repo", "/Users/you/src/app"]
    }
  }
}
```

### Default tools (coding agents)

```text
quality_context   quality_plan   quality_run   quality_status
quality_verify    quality_explain   quality_evidence
```

Example agent workflow:

1. `quality_context` — what obligations exist for this change  
2. `quality_plan` — gaps vs existing proofs (no execution)  
3. `quality_run` — execute impacted protection  
4. `quality_verify` — composite verdict  
5. `quality_explain` / `quality_evidence` — drill into a handle or id  

### Authoring profile (TestProgram draft → preview → promote)

```sh
npx @weavatrix/wvq@0.1.0-alpha.1 mcp --repo . \
  --profile authoring \
  --change checkout-fix \
  --base origin/main \
  --head WORKTREE
```

```text
quality_test_draft    quality_test_validate   quality_test_preview
quality_test_promote  quality_test_record     quality_test_heal
```

`doctor` / `init` / `baseline` stay **CLI-only** so agents cannot treat discovery
or an observed baseline as authority.

More configs and JSON-RPC samples: [examples/mcp/](examples/mcp/).

---

## JavaScript library cookbook

```sh
npm install --save-dev @weavatrix/wvq@0.1.0-alpha.1
```

### Plan → run → verify

```js
import { WvqClient } from '@weavatrix/wvq'

const wvq = new WvqClient({ repo: process.cwd() })

const plan = await wvq.plan({ change: 'checkout-fix' })
console.log(plan.obligations, plan.gaps)

const run = await wvq.run({
  change: 'checkout-fix',
  base: 'origin/main',
  head: 'WORKTREE',
  scope: 'impacted',
  evidencePolicy: 'minimal',
})
console.log(run.run_id, run.outcome, run.scope_reason)

const verify = await wvq.verify({ change: 'checkout-fix' })
console.log(verify.state, verify.quality.proof, verify.base, verify.head)

if (verify.blocking) {
  for (const reason of verify.quality.blocking_reasons) {
    console.error(reason.code, reason.detail)
  }
  process.exit(2)
}
```

### Spec / select / debt / explain

```js
await wvq.specValidate({ change: 'checkout-fix' })
const selected = await wvq.select({
  change: 'checkout-fix',
  base: 'origin/main',
  head: 'HEAD',
})
console.log(selected.selected, selected.uncovered_mandatory)

const debt = await wvq.debt({
  change: 'checkout-fix',
  base: 'origin/main',
  head: 'HEAD',
})
console.log({ new: debt.new, fixed: debt.fixed, returned: debt.returned })

const detail = await wvq.explain(verify.proofs[0]?.id)
console.log(detail.summary, detail.provenance)
```

### MCP client from Node (default + authoring)

```js
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const mcp = new WvqMcpClient({
  repo: process.cwd(),
  profile: 'default',
  change: 'checkout-fix',
})
const status = await mcp.call('quality_status', { change: 'checkout-fix' })
const verdict = await mcp.call('quality_verify', { change: 'checkout-fix' })

const authoring = new WvqMcpClient({
  repo: process.cwd(),
  profile: 'authoring',
  change: 'checkout-fix',
  base: 'origin/main',
  head: 'WORKTREE',
})
const draft = await authoring.draft()
const validated = await authoring.validate(candidateProgram)
const preview = await authoring.preview(validated.program, {
  screenshot: true,
  trace: true,
})
if (preview.passed) {
  await authoring.promote(preview.preview_id, validated.program)
}
```

### CI snippet (GitHub Actions)

```yaml
- uses: actions/setup-node@v4
  with:
    node-version: 24
- run: npx @weavatrix/wvq@0.1.0-alpha.1 doctor
- run: >
    npx @weavatrix/wvq@0.1.0-alpha.1 run
    --change current --base origin/${{ github.base_ref }} --head HEAD
    --scope impacted --evidence-policy minimal
- run: npx @weavatrix/wvq@0.1.0-alpha.1 verify --change current --observe-only true
```

Runnable copies: [examples/js/](examples/js/) · [examples/cli/](examples/cli/).

---

## Repository policy

`.weavatrix-quality/config.yaml` binds concrete tests to obligation IDs.
A green suite **without** this mapping stays `UNPROVEN`.

```yaml
quality_policy_v: 1

test_bindings:
  - path: tests/permissions.spec.ts
    runner: playwright
    case: viewer cannot delete a widget
    obligations: [permissions-delete]
    cost: 10
    flake_penalty: 0

browser:
  base_url: http://127.0.0.1:3000
  engine: chromium
  headless: true
  module_root: .
```

`wvq init` writes a fail-closed starter. This repo dogfoods change
`wvq-invariants` — see [examples/cli/dogfood.sh](examples/cli/dogfood.sh).

Browser preview needs Playwright in the **target** repo:

```sh
npm install --save-dev playwright
npx playwright install chromium
```

---

## What WVQ is / is not

| Is | Is not |
| --- | --- |
| Spec-to-Proof over existing runners | A second test framework |
| Weavatrix-backed impact + selection | A browser engine (Playwright stays) |
| CLI + MCP + typed JS boundary | An OpenSpec fork |
| 0 LLM tokens on the green path | Automatic silent oracle healing |
| Fail-closed on unknown schemas | A single global “quality %” |

Place in the ecosystem:

```text
Weavatrix          UNDERSTAND   what exists in source
Weavatrix Quality  PROVE        what must still be true after this change
Weavatrix Loom     COMPOSE      capabilities into ordinary Rust
```

---

## Build from source

Requires Rust **1.89+**.

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

---

## Docs map

| Doc | Use when |
| --- | --- |
| [examples/](examples/) | Copy-paste CLI / MCP / JS |
| [CHANGELOG](CHANGELOG.md) | What alpha ships |
| [docs/STATUS.md](docs/STATUS.md) | Maturity matrix + next task |
| [docs/adr/0003-…](docs/adr/0003-alpha-orchestration-gate.md) | Alpha vs deferred v1 |
| [docs/CANONICAL-MASTER-SPEC.md](docs/CANONICAL-MASTER-SPEC.md) | Normative product design |

## License

MIT. See [LICENSE](LICENSE).
