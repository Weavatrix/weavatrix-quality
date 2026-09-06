# wvq-bench

Shadow **impacted-vs-full** evaluation harness for
[Weavatrix Quality](https://github.com/Weavatrix/weavatrix-quality).

Alpha `0.1.0-alpha.1`. Not a Criterion microbench suite — it runs the real
`LiveService` path twice and compares failing test identities.

## Install

```sh
cargo install wvq-bench --version 0.1.0-alpha.1
# or
npx @weavatrix/wvq@0.1.0-alpha.1 bench --help
```

## Examples

```sh
wvq-bench \
  --repo /path/to/app \
  --change checkout-fix \
  --base origin/main \
  --head WORKTREE \
  --evidence-policy minimal

npx @weavatrix/wvq@0.1.0-alpha.1 bench \
  --repo . \
  --change current \
  --base origin/main \
  --head HEAD
```

### What you get

- Wall-clock and evidence-byte comparison for impacted vs full scopes
- Whether impacted selection **widened** to all
- A `selection-audit` when the full run finds a failure the impacted set missed
  (feeds future selection for that surface)

### When to use it

```text
CI dogfood / calibration   → measure selection quality on real fixtures
Debugging “why widened?”   → see scope_reason with both scopes executed
Not for                    → micro-optimizing Rust hot paths
```

## Related

- CLI: [`wvq-cli`](https://crates.io/crates/wvq-cli)
- MCP: [`wvq-mcp`](https://crates.io/crates/wvq-mcp)
- Examples: https://github.com/Weavatrix/weavatrix-quality/tree/main/examples

## License

MIT
