# Hard invariants

These are non-negotiable. If a shortcut violates one, the shortcut is wrong.

## Product

1. **AI is the compiler and exception handler, never the normal execution runtime.**
2. Routine green-path verification uses **0 runtime LLM tokens**.
3. The first-class result is a revision-bound `Proof`, not a test file and not a quality percentage.
4. Humans see unresolved exceptions, not hundreds of green cases.

## Authority split

5. OpenSpec owns intended externally visible behavior.
6. `weavatrix-rust` owns repository/code facts. WVQ must not create a second code graph.
7. Playwright owns the browser process. WVQ does not build a browser.
8. WVQ owns what must be proven: obligations, OracleSeal, TestProgram, debt ratchet, BehaviorGraph, Evidence Ledger, Proof, protection continuity.

## Oracles and repair

9. Implementation evidence can **propose** intent; it cannot **establish** intent.
10. Recovered requirements start `RECOVERED` and cannot enter `OracleSeal` until QA verification succeeds.
11. Automatic repair may change locators, waits, fixture plumbing, runner syntax. It may **not** change expected business results, permissions, invariants, or whether a behavior exists.
12. A contradiction to a sealed oracle is a regression or spec decision, not a healing opportunity.
13. `OBSERVED_ONLY` may be baseline evidence. It cannot become a normative seal.

## Evidence

14. Missing evidence is never evidence of absence.
15. Fail closed on unknown schema versions, unknown quality actions, invalid evidence, ambiguous revision identity, and sealed-oracle mutation.
16. Every result carries repository/revision provenance.
17. Large artifacts are handles. Screenshots/HAR/video do not enter an LLM context by default.

## Change analysis

18. Never compute impact only on head:
    `Impact(base) ∪ Impact(head) ∪ removed nodes/edges/public surfaces`.
19. Global coverage improvement must never override a local protection loss.
20. Existing quality debt can be baselined. New debt is classified separately. Old debt does not force a repo-wide cleanup before adoption.
21. Risk is `RiskEvidence[]`, never an opaque `risk=87%`.
22. Debt states: `Existing | New | Fixed | Returned | Excepted | Warning | ApproachingBudget`.

## Execution

23. No arbitrary shell command over MCP.
24. Unknown executor IDs fail. Commands are registered, bounded, and typed.
25. JS/TS/Bun and Go are first-class v1 target ecosystems.
26. WVQ does not invent replacement test runners.
27. XPath is not a default UI identity.

## Proof verdicts

Never collapse these:

```text
PROVEN
CONTRADICTED
PARTIAL
UNPROVEN
HUMAN_REQUIRED
```
