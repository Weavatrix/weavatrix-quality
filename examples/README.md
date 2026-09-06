# Examples — Weavatrix Quality

Copy-paste oriented samples for **alpha `0.1.0-alpha.1`**.

| Path | What |
| --- | --- |
| [cli/](cli/) | Shell recipes: doctor → run → verify, dogfood, CI |
| [mcp/](mcp/) | Cursor / Claude Desktop configs + tool recipes |
| [js/](js/) | `WvqClient` + `WvqMcpClient` scripts |

Install once:

```sh
npm install --save-dev @weavatrix/wvq@0.1.0-alpha.1
# or use npx without installing
```

From a repo that already has OpenSpec + `.weavatrix-quality/config.yaml`
(this repository dogfoods `wvq-invariants`):

```sh
# CLI
bash examples/cli/quickstart.sh
bash examples/cli/dogfood.sh

# JS (Node 20+)
node examples/js/plan-run-verify.mjs
node examples/js/mcp-default.mjs
```

Product overview: [../README.md](../README.md).
