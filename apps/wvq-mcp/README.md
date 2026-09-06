# wvq-mcp

Bounded **MCP** host for [Weavatrix Quality](https://github.com/Weavatrix/weavatrix-quality),
built on [`mcport`](https://crates.io/crates/mcport).

Alpha `0.1.0-alpha.2`. Registry name: **`io.github.Weavatrix/weavatrix-quality`**.

Most users should launch via npm:

```sh
npx @weavatrix/wvq@0.1.0-alpha.2 mcp --repo .
```

## Install

```sh
cargo install wvq-mcp --version 0.1.0-alpha.2
wvq-mcp --repo /path/to/app
```

## Cursor / Claude config

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "npx",
      "args": [
        "-y",
        "@weavatrix/wvq@0.1.0-alpha.2",
        "mcp",
        "--repo",
        "/absolute/path/to/repo"
      ]
    }
  }
}
```

Or point at the cargo-installed binary:

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "wvq-mcp",
      "args": ["--repo", "/absolute/path/to/repo"]
    }
  }
}
```

## Default profile (7 tools)

```text
quality_context   â€” obligations + bounded code context
quality_plan      â€” gaps vs existing proofs (no execution)
quality_run       â€” execute impacted / all protection
quality_status    â€” latest run status + handles
quality_verify    â€” composite change verdict
quality_explain   â€” provenance for an id
quality_evidence  â€” fetch artifact by handle
```

### Example agent prompts

```text
Use quality_context for change "checkout-fix" with purpose "implementation".
Then quality_plan. If gaps remain, quality_run with base origin/main and head WORKTREE.
Finally quality_verify and quality_explain on any blocking id.
```

## Authoring profile

Fixed at process start to one change + Git range:

```sh
wvq-mcp --repo . \
  --profile authoring \
  --change checkout-fix \
  --base origin/main \
  --head WORKTREE
```

```text
quality_test_draft     quality_test_validate   quality_test_preview
quality_test_promote   quality_test_record     quality_test_heal
```

```json
{
  "mcpServers": {
    "wvq-authoring": {
      "command": "npx",
      "args": [
        "-y",
        "@weavatrix/wvq@0.1.0-alpha.2",
        "mcp",
        "--repo", ".",
        "--profile", "authoring",
        "--change", "checkout-fix",
        "--base", "origin/main",
        "--head", "WORKTREE"
      ]
    }
  }
}
```

## Guarantees

- No arbitrary shell over MCP
- Large artifacts return as **handles**
- Fail closed on unknown schemas / actions
- `doctor` / `init` / `baseline` are **not** default MCP tools

## JS helper

```js
import { WvqMcpClient } from '@weavatrix/wvq/mcp'

const mcp = new WvqMcpClient({ repo: process.cwd(), change: 'checkout-fix' })
await mcp.call('quality_verify', { change: 'checkout-fix' })
```

More samples: https://github.com/Weavatrix/weavatrix-quality/tree/main/examples/mcp

## License

MIT
