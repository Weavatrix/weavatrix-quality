# wvq-cli

Command-line surface for **[Weavatrix Quality](https://github.com/Weavatrix/weavatrix-quality)** (`wvq`).

Alpha `0.1.0-alpha.2` â€” crates and APIs may change before 1.0.
**Primary install for most users is npm:** [`@weavatrix/wvq`](https://www.npmjs.com/package/@weavatrix/wvq).

## Install

```sh
cargo install wvq-cli --version 0.1.0-alpha.2
# or
npx @weavatrix/wvq@0.1.0-alpha.2 --help
```

The published binary name is `wvq`.

## Examples

```sh
wvq --repo /path/to/app doctor
wvq --repo /path/to/app init
wvq --repo /path/to/app spec validate --change checkout-fix
wvq --repo /path/to/app plan --change checkout-fix

wvq --repo /path/to/app run \
  --change checkout-fix \
  --base origin/main \
  --head HEAD \
  --scope impacted \
  --evidence-policy minimal

wvq --repo /path/to/app status
wvq --repo /path/to/app verify --change checkout-fix
wvq --repo /path/to/app verify --change checkout-fix --observe-only true
wvq --repo /path/to/app explain <id>

wvq --repo /path/to/app select --change checkout-fix --base origin/main --head WORKTREE
wvq --repo /path/to/app debt --change checkout-fix --base origin/main --head HEAD
wvq --repo /path/to/app record --change checkout-fix --route /checkout
```

### Typical agent / human loop

```text
doctor â†’ spec validate â†’ plan â†’ run (impacted) â†’ verify â†’ explain
```

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | ok / observe-only |
| 1 | unresolved evidence / failure |
| 2 | blocking contradicted verify |

## Related crates

| Crate | Role |
| --- | --- |
| [`wvq-mcp`](https://crates.io/crates/wvq-mcp) | MCP host |
| [`wvq-bench`](https://crates.io/crates/wvq-bench) | Impacted-vs-full shadow |
| [`qualityd`](https://crates.io/crates/qualityd) | Local Studio HTTP |

Library crates in this family are **unstable alpha** â€” do not treat them as a
stable public API yet.

## Docs & examples

- Product README: https://github.com/Weavatrix/weavatrix-quality#readme
- Copy-paste examples: https://github.com/Weavatrix/weavatrix-quality/tree/main/examples
- CHANGELOG: https://github.com/Weavatrix/weavatrix-quality/blob/main/CHANGELOG.md

## License

MIT
