# wvq-mcp

Bounded MCP host for **Weavatrix Quality**, built on [mcport](https://crates.io/crates/mcport).

```sh
cargo install wvq-mcp --version 0.1.0-alpha.1
wvq-mcp --repo .
# or
npx wvq@0.1.0-alpha.1 mcp --repo .
```

Default tools: `quality_context`, `quality_plan`, `quality_run`, `quality_status`,
`quality_verify`, `quality_explain`, `quality_evidence`.

No arbitrary shell. Large artifacts return as handles. Registry name:
`io.github.Weavatrix/weavatrix-quality`.

See [examples](https://github.com/Weavatrix/weavatrix-quality/tree/main/examples)
for Cursor MCP config snippets.
