# wvq-bench

Shadow selected-vs-full evaluation harness for **Weavatrix Quality**.

```sh
cargo install wvq-bench --version 0.1.0-alpha.1
wvq-bench --repo . --change current --base origin/main --head WORKTREE
# or
npx @weavatrix/wvq@0.1.0-alpha.1 bench --repo . --change current --base origin/main --head WORKTREE
```

Compares impacted vs full failing identities through `LiveService`. Not a
Criterion microbenchmark suite.

Alpha / unstable: API and CLI flags may change before 1.0.
