# JS examples

Requires Node 20+ and `@weavatrix/wvq@0.1.0-alpha.1` (or a local `npm link`).

```sh
npm install --save-dev @weavatrix/wvq@0.1.0-alpha.1

node examples/js/plan-run-verify.mjs
WVQ_CHANGE=wvq-invariants node examples/js/plan-run-verify.mjs

node examples/js/select-debt-explain.mjs
node examples/js/mcp-default.mjs

# authoring needs Playwright in the target repo + a candidate program JSON
WVQ_CHANGE=current CANDIDATE_PROGRAM_JSON=./candidate.json \
  node examples/js/mcp-authoring.mjs
```

| Script | API |
| --- | --- |
| `plan-run-verify.mjs` | `WvqClient` plan / run / verify |
| `select-debt-explain.mjs` | select, debt, observe-only verify, explain |
| `mcp-default.mjs` | `WvqMcpClient` default tools |
| `mcp-authoring.mjs` | draft → validate → preview → promote → heal |

Env vars: `WVQ_REPO`, `WVQ_CHANGE`, `WVQ_BASE`, `WVQ_HEAD`.
