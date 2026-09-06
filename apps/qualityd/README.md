# qualityd

Local HTTP **Quality Studio** for
[Weavatrix Quality](https://github.com/Weavatrix/weavatrix-quality).

Alpha `0.1.0-alpha.1`. Exception-first cockpit over the same command bus as
`wvq` / `wvq-mcp` — not a second policy engine.

## Install

```sh
cargo install qualityd --version 0.1.0-alpha.1
qualityd --repo /path/to/app
```

Prefer npm for the CLI/MCP binaries (`@weavatrix/wvq`). Use `qualityd` when you
want the local Studio UI/API.

## Examples

```sh
# start Studio (default binds locally)
qualityd --repo .

# then drive the same change from CLI
npx @weavatrix/wvq@0.1.0-alpha.1 run \
  --change current --base origin/main --head HEAD --scope impacted
npx @weavatrix/wvq@0.1.0-alpha.1 verify --change current
```

Authoring HTTP endpoints (same semantics as MCP authoring tools):

```text
POST /api/v1/authoring/draft
POST /api/v1/authoring/validate
POST /api/v1/authoring/preview
POST /api/v1/authoring/record
POST /api/v1/authoring/promote
POST /api/v1/authoring/heal
```

Studio summary debt uses the **same stored run range** as `verify` (never a
hardcoded `HEAD`/`WORKTREE` pair).

## Related

- [`wvq-cli`](https://crates.io/crates/wvq-cli) · [`wvq-mcp`](https://crates.io/crates/wvq-mcp)
- Product README: https://github.com/Weavatrix/weavatrix-quality#readme

## License

MIT
