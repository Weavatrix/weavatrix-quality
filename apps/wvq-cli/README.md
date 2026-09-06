# wvq (CLI)

Binary crate for **Weavatrix Quality**.

```sh
cargo install wvq-cli --version 0.1.0-alpha.1
# or
npx wvq@0.1.0-alpha.1 --help
```

```sh
wvq doctor
wvq run --change current --base origin/main --head HEAD --scope impacted
wvq verify --change current
```

Primary distribution for end users is the npm package `wvq`, which ships this
native binary for six platforms. Library crates in this workspace are **unstable
alpha** — do not embed them as a stable public API yet.

See the [product README](https://github.com/Weavatrix/weavatrix-quality#readme)
and [CHANGELOG](https://github.com/Weavatrix/weavatrix-quality/blob/main/CHANGELOG.md).
