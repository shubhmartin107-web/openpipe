# OpenPipe

**Unify dbt-style SQL/Python transformation with a lightweight DAG scheduler.**

Built to replace dbt Cloud + Airflow/Dagster with a single tool that runs identically on a laptop or a cluster.

## Features

- **dbt-Compatible SQL** — `{{ ref() }}`, `{{ source() }}`, `{{ config() }}`, `{% is_incremental() %}`, Jinja2
- **Python Model Support** — `.py` files with `dbt.ref()` / `dbt.source()` compiled to PySpark DataFrame operations
- **Lightweight DAG Scheduler** — Go-based, sub-millisecond scheduling latency, cron + event/webhook triggers
- **Materializations** — View, table, incremental (merge), ephemeral
- **Column-Level Lineage** — Automatic SQL AST analysis (JOIN/CTE/subquery/aggregation/wildcard), exported as OpenLineage `ColumnLineageDatasetFacet`
- **Data Tests** — not_null, unique, accepted_values, relationships, custom SQL (COUNT(*) AS failures pattern)
- **Backfill Cascade** — Full refresh model + all downstream dependents automatically
- **Webhook Triggers** — Event-driven pipeline execution with secret validation
- **Lakehouse Native** — Executes against Spark, Trino, DuckDB, or stdout (for debugging)
- **Arrow Interchange** — Columnar data format between layers (Parquet/IPC/CSV output)
- **MCP Server** — AI-accessible pipeline management (Model Context Protocol, 6 tools)
- **Visual DAG** — Real-time DAG visualization with lineage side panel (React/React Flow)
- **Single Binary** — Go scheduler 11MB static, Rust engine 12MB static
- **Test Runner** — Compile + execute test suites against any SQL engine
- **CI/CD Ready** — Built-in GitHub Actions workflow, integration test suite, benchmark scripts

## Architecture

```
┌─────────────────────────────────────────┐
│         openpipe-scheduler (Go)         │
│  REST API :8080  |  MCP :8081          │
│  DAG Engine → Cron → Executor          │
│  Webhooks → Backfill → ArrowWriter     │
└──────────────────┬──────────────────────┘
                   │ HTTP
┌──────────────────▼──────────────────────┐
│         openpipe-engine (Rust)          │
│  SQL Compiler → Lineage Analyzer        │
│  (minijinja + sqlparser-rs)            │
│  Test Compiler → OpenLineage Export     │
└──────────────────┬──────────────────────┘
                   │ Compiled SQL
┌──────────────────▼──────────────────────┐
│       Lakehouse SQL Engine (Spark)      │
│  Trino / DuckDB / Iceberg / Delta Lake  │
└─────────────────────────────────────────┘
```

## Quick Start

```bash
# Prerequisites: Rust, Go 1.24+, Spark/Trino/Iceberg, or set SQL_DRIVER=stdout

# 1. Build
cargo build --release -p openpipe-engine
cd go-scheduler && go build -o openpipe-scheduler ./cmd/openpipe

# 2. Start engine
OPENPIPE_PROJECT_PATH=./demo/project ./target/release/openpipe-engine &

# 3. Start scheduler (stdout driver logs SQL without executing)
OPENPIPE_SQL_DRIVER=stdout ./go-scheduler/openpipe-scheduler &

# 4. Run entire pipeline
curl -X POST http://localhost:8080/api/v1/runs \
  -H "Content-Type: application/json" \
  -d '{"full_refresh": true}'

# 5. View DAG
curl http://localhost:8080/api/v1/dag

# 6. View column-level lineage
curl http://localhost:8080/api/v1/lineage

# 7. Backfill with cascade (model + downstream)
curl -X POST http://localhost:8080/api/v1/backfill \
  -H "Content-Type: application/json" \
  -d '{"model_name": "fct_orders"}'

# 8. Register a webhook trigger
curl -X POST http://localhost:8080/api/v1/webhooks/register \
  -H "Content-Type: application/json" \
  -d '{"name": "order_webhook", "model_name": "stg_orders", "secret": "mysecret"}'

# 9. Trigger the webhook
curl -X POST "http://localhost:8080/api/v1/webhook?event=new_order" \
  -H "X-Webhook-Secret: mysecret" \
  -d '{"order_id": 123}'
```

## Demo

See `demo/` for a complete star schema project:
- 6 SQL models (3 staging, 1 dimension, 1 fact, 1 mart)
- 3 raw sources with column-level tests
- 14 tests (not_null, unique, accepted_values, relationships, custom)
- Column-level lineage from raw → staging → mart
- Python model examples
- Docker Compose with Spark + MinIO + OpenPipe

## API Endpoints

### Rust Engine (:9090)
| Method | Route | Description |
|--------|-------|-------------|
| GET | `/api/v1/health` | Health check |
| POST | `/api/v1/project/load` | Load project YAML |
| POST | `/api/v1/compile` | Compile all/models (Jinja2 → SQL) |
| POST | `/api/v1/lineage` | Column-level lineage analysis |
| POST | `/api/v1/lineage/openlineage` | OpenLineage 1.x event export |
| POST | `/api/v1/tests/compile` | Compile data tests to SQL |
| POST | `/api/v1/tests/suite` | Test suite with pass/fail summary |
| GET | `/api/v1/models/types` | Model languages (SQL/Python) + materializations |

### Go Scheduler (:8080)
| Method | Route | Description |
|--------|-------|-------------|
| GET | `/api/v1/health` | Health check + engine status |
| GET | `/api/v1/runs` | List all runs |
| POST | `/api/v1/runs` | Trigger a pipeline run |
| GET | `/api/v1/runs/{runID}` | Get run details |
| GET | `/api/v1/dag` | Compiled DAG with edges |
| GET | `/api/v1/models` | List models with config |
| GET | `/api/v1/lineage` | Column-level lineage |
| POST | `/api/v1/backfill` | Full refresh with cascade |
| POST | `/api/v1/webhook` | Event-driven pipeline trigger |
| POST | `/api/v1/webhooks/register` | Register webhook handler |
| POST | `/api/v1/webhooks/unregister/{name}` | Remove webhook handler |
| GET | `/api/v1/schedules` | List cron schedules |
| POST | `/api/v1/schedules` | Create cron schedule |
| DELETE | `/api/v1/schedules/{name}` | Remove cron schedule |

### MCP Server (:8081)
See [MCP.md](./MCP.md) for the 6 available tools:
`pipe_run`, `pipe_compile`, `pipe_lineage`, `pipe_list_runs`, `pipe_get_run`, `pipe_health`

## Components

| Component | Language | Role |
|-----------|----------|------|
| `rust-engine/` | Rust | SQL/Python compilation, Jinja2 eval, column-level lineage, OpenLineage export, test runner |
| `go-scheduler/` | Go | DAG orchestration, cron scheduling, SQL execution, webhooks, backfill cascade, REST API, MCP server |
| `web/` | TypeScript | React/React Flow DAG visualization, lineage explorer, run history |
| `demo/` | SQL/YAML | Star schema demo project (6 models, 3 sources, 14 tests) |
| `.github/workflows/` | YAML | CI/CD (Rust + Go + Web + integration tests) |

## Testing

```bash
# Unit tests (Rust + Go)
./run-tests.sh

# Integration tests (starts engine, hits all HTTP endpoints)
./test-integration.sh

# Benchmarks (compile latency, DAG build, binary sizes)
./run-benchmarks.sh
```

## Scripts

| Script | Purpose |
|--------|---------|
| `run-tests.sh` | Fast pre-commit check: cargo check + cargo test + go build + go vet + go test |
| `test-integration.sh` | Full E2E: builds → unit tests → HTTP API (8 endpoints) → DAG edge count → all pass |
| `run-benchmarks.sh` | DAG build time, compile latency, lineage extraction, test compile, binary sizes |

## MCP Server

OpenPipe exposes an MCP server on `:8081` for AI-assisted pipeline management.
See [MCP.md](./MCP.md) for available tools.

## License

Apache 2.0
