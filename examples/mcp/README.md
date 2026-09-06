# MCP client configs and tool recipes for Weavatrix Quality.
#
# Launchers:
#   npx @weavatrix/wvq@0.1.0-alpha.3 mcp --repo <abs-path>
#   cargo install wvq-mcp && wvq-mcp --repo <abs-path>
#
# Registry: io.github.Weavatrix/weavatrix-quality

## Files

| File | Use |
| --- | --- |
| [cursor.mcp.json](cursor.mcp.json) | Cursor `mcp.json` (default 7 tools) |
| [claude-desktop.json](claude-desktop.json) | Claude Desktop config |
| [authoring.mcp.json](authoring.mcp.json) | Authoring profile (TestProgram loop) |
| [tool-calls.md](tool-calls.md) | Example tool arguments / agent scripts |

## Default tools

```text
quality_context  quality_plan  quality_run  quality_status
quality_verify   quality_explain  quality_evidence
```

## Authoring tools

```text
quality_test_draft  quality_test_validate  quality_test_preview
quality_test_promote  quality_test_record  quality_test_heal
```

`wvq doctor`, `wvq init`, and `wvq baseline` are **CLI-only**.
