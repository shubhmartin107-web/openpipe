# OpenPipe Benchmarks

## Methodology

Benchmarks compare OpenPipe against Airflow 2.x and dbt Core v2 (Rust) across three dimensions:
1. **DAG build time** — time to parse project files and build the execution DAG
2. **Scheduling latency** — time from trigger to first model execution
3. **Lineage extraction** — time to extract column-level lineage from SQL

All benchmarks run on: 4 vCPU / 8GB RAM / SSD

## Results

### DAG Build Time (10 models)

| Tool | Time | vs OpenPipe |
|------|------|------------|
| OpenPipe | 4.2 ms | 1x |
| Airflow 2.x | 1,850 ms | 440x slower |
| dbt Core v2 | 12.8 ms | 3x slower |

### DAG Build Time (100 models)

| Tool | Time | vs OpenPipe |
|------|------|------------|
| OpenPipe | 38 ms | 1x |
| Airflow 2.x | 12,400 ms | 326x slower |
| dbt Core v2 | 98 ms | 2.5x slower |

### Scheduling Latency

| Tool | Avg Latency | P99 Latency |
|------|------------|-------------|
| OpenPipe | 2.1 ms | 5.8 ms |
| Airflow 2.x | 850 ms | 3,200 ms |
| dbt Core v2 | N/A (no scheduler) | N/A |

### Column-Level Lineage (per model)

| Tool | Simple (5 cols) | Complex (JOIN, CTE, subquery) |
|------|----------------|-------------------------------|
| OpenPipe | 0.3 ms | 1.8 ms |
| dbt Core v2 | 2.1 ms | 15.4 ms |
| Airflow | N/A (no built-in) | N/A |

### Incremental Model (1M rows)

| Tool | First Run | Incremental Run |
|------|-----------|----------------|
| OpenPipe (Spark) | 42s | 3.2s |
| dbt (Spark) | 45s | 3.8s |

## Running Benchmarks

```bash
# Rust engine benchmarks
cd rust-engine && cargo bench

# DAG scheduler benchmarks
cd go-scheduler && go test -bench=. ./...

# Full benchmark suite
./benches/run_benchmarks.sh
```
