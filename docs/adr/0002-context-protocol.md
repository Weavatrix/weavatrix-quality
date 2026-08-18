# ADR 0002 — Context protocol for long-running implementation

Status: accepted
Date: 2026-08-18

## Context

The canonical spec is large (~90 sections). Implementing it across many agent sessions will lose decisions if the only memory is chat.

## Decision

1. The spec is copied into the repo and treated as law.
2. `docs/STATUS.md` is the live cursor. Every session updates it.
3. Agents load only the spec sections listed in STATUS.
4. One spec task = one commit = one STATUS update.
5. Divergences become ADRs, not silent edits of the spec.
6. Crates are added to the workspace only when their first task starts.

## Consequences

- A new agent can start from `AGENTS.md` + `STATUS.md` without the original conversation.
- We accept slower “one task per turn” in exchange for a bisectable history.
- The Downloads copy of the spec is no longer authoritative.
