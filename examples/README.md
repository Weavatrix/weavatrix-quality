# Examples — Weavatrix Quality alpha

These snippets assume `wvq@0.1.0-alpha.1` (npm) or a local `cargo build -p wvq-cli`.

## CLI: plan → run → verify

```sh
# Discover what is already configured (read-only)
npx wvq@0.1.0-alpha.1 doctor

# Compile OpenSpec obligations for a change
npx wvq@0.1.0-alpha.1 spec validate --change wvq-invariants

# Impacted execution (existing runners only)
npx wvq@0.1.0-alpha.1 run \
  --change wvq-invariants \
  --base origin/main \
  --head HEAD \
  --scope impacted \
  --evidence-policy minimal

# Read-only composite verdict from stored evidence
npx wvq@0.1.0-alpha.1 verify --change wvq-invariants
```

## MCP (Cursor / Claude Desktop)

`wvq-mcp` is an mcport host. Add to your MCP client config:

```json
{
  "mcpServers": {
    "weavatrix-quality": {
      "command": "npx",
      "args": ["-y", "wvq@0.1.0-alpha.1", "mcp", "--repo", "."]
    }
  }
}
```

Default tools: `quality_context`, `quality_plan`, `quality_run`, `quality_status`,
`quality_verify`, `quality_explain`, `quality_evidence`.

## JavaScript client

See [`js-client.mjs`](js-client.mjs).
