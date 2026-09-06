# Example MCP tool calls

Replace `checkout-fix` with your OpenSpec change folder name.

## Default profile â€” agent script

```text
1. quality_context
   { "change": "checkout-fix", "purpose": "implementation", "token_budget": 4000 }

2. quality_plan
   { "change": "checkout-fix" }

3. quality_run
   {
     "change": "checkout-fix",
     "base": "origin/main",
     "head": "WORKTREE",
     "scope": "impacted",
     "evidence_policy": "minimal"
   }

4. quality_status
   {}

5. quality_verify
   { "change": "checkout-fix" }

6. quality_explain
   { "id": "<proof-or-finding-id-from-verify>" }

7. quality_evidence
   { "handle": "<cas-handle-from-run-or-verify>" }
```

## Authoring profile â€” TestProgram loop

Process must be started with `--profile authoring --change â€¦ --base â€¦ --head â€¦`.

```text
1. quality_test_draft
   { "token_budget": 8000, "use_model": false }

2. quality_test_validate
   { "program": { /* candidate TestProgram JSON */ } }

3. quality_test_preview
   { "program": { /* validated program */ }, "screenshot": true, "trace": true }

4. quality_test_promote   (only if preview.passed)
   { "preview_id": "<id>", "program": { /* same validated program */ } }

5. quality_test_record
   {
     "route": "/checkout",
     "fixture_values": { "email": "demo@example.test" },
     "idle_timeout_ms": 3000,
     "max_events": 200
   }

6. quality_test_heal
   {
     "program_id": "<id>",
     "expected_program_revision": 1,
     "edits": [
       { "edit": "insert_wait", "after": 0, "condition": { "kind": "url", "route": "/ready" } }
     ],
     "screenshot": true,
     "trace": false
   }
```

## Stdio smoke (JSON-RPC)

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}' \
  | npx @weavatrix/wvq@0.1.0-alpha.2 mcp --repo .
```
