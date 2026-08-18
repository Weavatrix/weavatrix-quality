# ADR 0001 — Product identity and repository boundary

Status: accepted
Date: 2026-08-18

## Decision

The product is **Weavatrix Quality**.

| Slot | Value |
| --- | --- |
| Product name | Weavatrix Quality |
| Repository | `weavatrix-quality` |
| Short / crate prefix | WVQ / `wvq-*` |
| CLI | `wvq` |
| Config / store | `.weavatrix-quality/` |
| GitHub | `https://github.com/sergii-ziborov/weavatrix-quality` |

It is a **separate product** that embeds `weavatrix-rust`. It is not a crate inside `weavatrix`, `weavatrix-rust`, or `weavatrix-loom`.

## Why these names

- Matches the existing family: Weavatrix, Weavatrix Loom, Cortex Loom.
- The spec already uses `weavatrix-quality`, WVQ, and `wvq` consistently.
- “Quality” is the owned noun: proof, debt, protection — not “tests” or “QA agent”.

## Rejected names

- `weavatrix-qa` — job title, not product
- `weavatrix-test` — implies a runner
- `wv-quality` / `qualityrix` — breaks the family prefix
- Folding into `weavatrix` — would mix read-only code intelligence with execution, storage, and policy

## Consequences

- All public docs, crates, MCP tool prefixes (`quality_*`), and the Studio title use this identity.
- Future agents must not rename the repo mid-flight.
