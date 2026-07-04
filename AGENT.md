# OpenPipe Agent Guide

## Build

```bash
# Rust engine
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release -p openpipe-engine

# Go scheduler
cd go-scheduler && GONOSUMCHECK=* GONOSUMDB=* go build -o openpipe-scheduler ./cmd/openpipe

# Web UI
cd web && npm install && npm run build
```

## Test

```bash
# All tests (recommended)
./run-tests.sh

# Rust tests only
cargo test -p openpipe-engine

# Go tests only
cd go-scheduler && GONOSUMCHECK=* GONOSUMDB=* go test ./... -count=1 -v

# Integration tests (starts engine, runs HTTP API)
./test-integration.sh

# Benchmarks (compile latency, DAG build, binary size)
./run-benchmarks.sh
```

## Run

```bash
# 1. Start the Rust compilation engine
OPENPIPE_PROJECT_PATH=./demo/project ./target/release/openpipe-engine

# 2. Start the Go scheduler
OPENPIPE_ENGINE_URL=http://localhost:9090 \
  OPENPIPE_SQL_DRIVER=stdout \
  ./go-scheduler/openpipe-scheduler

# 3. Run a pipeline
curl -X POST http://localhost:8080/api/v1/runs \
  -H "Content-Type: application/json" \
  -d '{"full_refresh": true}'

# 4. View DAG
curl http://localhost:8080/api/v1/dag

# 5. View lineage
curl http://localhost:8080/api/v1/lineage

# 6. Backfill cascade (model + all downstream)
curl -X POST http://localhost:8080/api/v1/backfill \
  -H "Content-Type: application/json" \
  -d '{"model_name": "fct_orders"}'

# 7. Register a webhook handler
curl -X POST http://localhost:8080/api/v1/webhooks/register \
  -H "Content-Type: application/json" \
  -d '{"name": "incoming_orders", "model_name": "stg_orders", "secret": "mysecret"}'

# 8. Trigger a webhook event
curl -X POST "http://localhost:8080/api/v1/webhook?event=new_orders" \
  -H "Content-Type: application/json" \
  -H "X-Webhook-Secret: mysecret" \
  -d '{"order_id": 123, "status": "completed"}'

# 9. View model types (SQL vs Python)
curl http://localhost:9090/api/v1/models/types
```

## Architecture

```
openpipe/
├── rust-engine/        # SQL/Python compilation + column-level lineage (HTTP :9090)
├── go-scheduler/       # DAG scheduler + executor + MCP + webhooks (:8080 REST, :8081 MCP)
├── web/                # Visual DAG (React/React Flow)
├── demo/               # Star schema demo project (6 models, 3 sources, 7+ tests)
├── .github/workflows/  # CI/CD (Rust + Go + Web + Integration)
├── test-integration.sh # HTTP API integration test suite
├── run-benchmarks.sh   # Latency/build benchmarks
└── run-tests.sh        # Full test suite runner
```

## Key Concepts

- **Models**: SQL files with `{{ ref() }}` and `{{ source() }}` Jinja2 macros; `.py` files for Python models
- **Materializations**: `view`, `table`, `incremental`, `ephemeral`
- **Lineage**: Column-level SQL analysis exported as OpenLineage `ColumnLineageDatasetFacet`
- **Tests**: `not_null`, `unique`, `accepted_values`, `relationships`, custom SQL
- **Schedules**: Cron expressions with `/api/v1/schedules` CRUD
- **Webhooks**: Event-driven pipeline triggers with secret validation, registered via API
- **Backfills**: Full refresh with downstream cascade (auto-runs all dependents)
- **Arrow Interchange**: Columnar output via ArrowWriter (Parquet, Arrow IPC, CSV)
- **Python Models**: PySpark DataFrame operations with `.py` files, auto-wrapped with Spark.table()
- **MCP Server**: Model Context Protocol server on :8081 with `pipe_run/pipe_compile/pipe_lineage` tools
- **OpenLineage**: 1.x compatible column-level lineage events with `ColumnLineageDatasetFacet` facets
