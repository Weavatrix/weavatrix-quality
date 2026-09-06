# Changelog

## 0.1.0-alpha.3 — 2026-09-06

Docs publish fix: strip accidental UTF-8 BOM from `package.json` / release
text so the npm assemble step can parse JSON. Ships the same example-heavy
README surface as the aborted `0.1.0-alpha.2` npm attempt. No product behavior
change versus `0.1.0-alpha.1`.

### Distribution

- npm `@weavatrix/wvq@0.1.0-alpha.3`
- crates.io workspace closure `0.1.0-alpha.3`
- MCP Registry `io.github.Weavatrix/weavatrix-quality` @ `0.1.0-alpha.3`
- GitHub Release `v0.1.0-alpha.3`

## 0.1.0-alpha.2 — 2026-09-06

Docs-only alpha refresh intended for example-heavy READMEs. crates.io published;
npm assemble failed on UTF-8 BOM in `package.json` (fixed in alpha.3).

## 0.1.0-alpha.1 — 2026-09-06

First public alpha of Weavatrix Quality (WVQ). See ADR 0003 for the orchestration
gate and deferred v1 sequence.
