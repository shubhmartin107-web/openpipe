#!/usr/bin/env bash
set -euo pipefail

echo "=== OpenPipe Benchmarks ==="
echo ""

PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
ENGINE_BIN="${PROJECT_ROOT}/target/release/openpipe-engine"
SCHEDULER_BIN="${PROJECT_ROOT}/go-scheduler/openpipe"
DEMO_DIR="${PROJECT_ROOT}/demo/project"

# Ensure binaries exist
if [ ! -f "$ENGINE_BIN" ]; then
  echo "Building Rust engine..."
  (cd "$PROJECT_ROOT" && cargo build --release -p openpipe-engine)
fi
if [ ! -f "$SCHEDULER_BIN" ]; then
  echo "Building Go scheduler..."
  (cd "${PROJECT_ROOT}/go-scheduler" && go build -o "$SCHEDULER_BIN" ./cmd/openpipe)
fi

echo "1. DAG Build Time (Go scheduler)"
echo "---------------------------------"
# Build DAG from demo project
start=$(${PROJECT_ROOT}/scripts/time_ms.sh)
"$SCHEDULER_BIN" --project "$DEMO_DIR" --build-dag-only 2>/dev/null || true
end=$(${PROJECT_ROOT}/scripts/time_ms.sh)
echo "  DAG build: $((end - start)) ms"
echo ""

echo "2. Compilation Latency (Rust engine)"
echo "------------------------------------"
# Start engine
"$ENGINE_BIN" &
ENGINE_PID=$!
sleep 2

# Compile all models
start=$(date +%s%N)
curl -s -X POST http://localhost:9090/api/v1/project/load \
  -H "Content-Type: application/json" \
  -d "{\"path\": \"$DEMO_DIR\"}" > /dev/null
end=$(date +%s%N)
echo "  Project load: $(((end - start) / 1000000)) ms"

start=$(date +%s%N)
resp=$(curl -s -X POST http://localhost:9090/api/v1/compile \
  -H "Content-Type: application/json" \
  -d '{"full_refresh": true}')
end=$(date +%s%N)
nmodels=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['models']))")
echo "  Compile ($nmodels models): $(((end - start) / 1000000)) ms"

start=$(date +%s%N)
curl -s -X POST http://localhost:9090/api/v1/lineage \
  -H "Content-Type: application/json" \
  -d '{}' > /dev/null
end=$(date +%s%N)
echo "  Lineage: $(((end - start) / 1000000)) ms"

kill "$ENGINE_PID" 2>/dev/null || true
wait "$ENGINE_PID" 2>/dev/null || true
echo ""

echo "3. Test Compilation (Rust engine)"
echo "----------------------------------"
# Restart engine
"$ENGINE_BIN" &
ENGINE_PID=$!
sleep 2

curl -s -X POST http://localhost:9090/api/v1/project/load \
  -H "Content-Type: application/json" \
  -d "{\"path\": \"$DEMO_DIR\"}" > /dev/null

start=$(date +%s%N)
resp=$(curl -s -X POST http://localhost:9090/api/v1/tests/compile \
  -H "Content-Type: application/json" \
  -d '{}')
end=$(date +%s%N)
ntests=$(echo "$resp" | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d['tests']))")
echo "  Test compile ($ntests tests): $(((end - start) / 1000000)) ms"

kill "$ENGINE_PID" 2>/dev/null || true
wait "$ENGINE_PID" 2>/dev/null || true
echo ""

echo "4. Binary Size"
echo "--------------"
echo "  Rust engine: $(du -h "$ENGINE_BIN" | cut -f1)"
echo "  Go scheduler: $(du -h "$SCHEDULER_BIN" | cut -f1)"

echo ""
echo "=== Benchmarks complete ==="
