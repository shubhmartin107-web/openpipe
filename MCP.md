# OpenPipe MCP Server

OpenPipe exposes a Model Context Protocol (MCP) server for AI-assisted pipeline management.

## Endpoint

`http://localhost:8081/mcp`

## Tools

### `pipe_run`
Run a model or the entire project.

**Parameters:**
- `model` (string, optional) — Model name to run. Empty = run all.
- `full_refresh` (boolean, default: false) — Force full refresh of incremental models.

**Returns:** `{ run_id: string, status: string }`

### `pipe_compile`
Compile a project and return the DAG.

**Parameters:**
- `model` (string, optional) — Specific model to compile.

**Returns:** Compiled models with SQL, DAG edges, config.

### `pipe_lineage`
Get OpenLineage-compatible column-level lineage.

**Returns:** Column-level lineage for all models with transformation types.

### `pipe_list_runs`
List recent pipeline runs.

**Returns:** Array of run objects with status, timing, steps.

### `pipe_get_run`
Get details of a specific run.

**Parameters:**
- `run_id` (string, required) — Run ID.

### `pipe_health`
Check system health.

## Example (Claude Code)

```
> Run the stg_customers model with full refresh
```

This triggers `pipe_run({model: "stg_customers", full_refresh: true})`.

## Example (curl)

```bash
curl -X POST http://localhost:8081/mcp \
  -H "Content-Type: application/json" \
  -d '{"method": "call_tool", "params": {"name": "pipe_lineage"}}'
```

## Architecture

The MCP server runs inside the Go scheduler process and proxies to the Rust engine for compilation/lineage and the run manager for execution.
