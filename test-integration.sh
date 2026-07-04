#!/usr/bin/env bash
set -euo pipefail

export PATH="/tmp/go/bin:$HOME/.cargo/bin:$PATH"
export GONOSUMCHECK=* GONOSUMDB=*

echo "=== OpenPipe Integration Tests ==="
echo ""

PROJECT_ROOT="$(cd "$(dirname "$0")" && pwd)"
DEMO_DIR="${PROJECT_ROOT}/demo/project"
TMPDIR="$(mktemp -d)"
PASS=0
FAIL=0

check() {
  local name="$1"
  local cmd="$2"
  echo -n "  [$name] ... "
  if eval "$cmd" > "$TMPDIR/stdout" 2>&1; then
    echo "PASS"
    PASS=$((PASS + 1))
  else
    echo "FAIL"
    cat "$TMPDIR/stdout"
    FAIL=$((FAIL + 1))
  fi
}

pycheck() {
  local name="$1"
  local script="$2"
  local data_file="$3"
  echo -n "  [$name] ... "
  if python3 -c "$script" < "$data_file" > "$TMPDIR/stdout" 2>&1; then
    echo "PASS"
    PASS=$((PASS + 1))
  else
    echo "FAIL"
    cat "$TMPDIR/stdout"
    FAIL=$((FAIL + 1))
  fi
}

cleanup() {
  if [ -n "${ENGINE_PID:-}" ]; then
    kill "$ENGINE_PID" 2>/dev/null || true
    wait "$ENGINE_PID" 2>/dev/null || true
  fi
  rm -rf "$TMPDIR"
}
trap cleanup EXIT

echo "1. Build binaries"
echo "-----------------"
(cd "$PROJECT_ROOT" && cargo build --release -p openpipe-engine 2>&1)
(cd "${PROJECT_ROOT}/go-scheduler" && go build -o openpipe-scheduler ./cmd/openpipe 2>&1)
check "Rust engine binary" "test -f '${PROJECT_ROOT}/target/release/openpipe-engine'"
check "Go scheduler binary" "test -f '${PROJECT_ROOT}/go-scheduler/openpipe-scheduler'"

echo ""
echo "2. Rust unit tests"
echo "-------------------"
(cd "$PROJECT_ROOT" && cargo test -p openpipe-engine 2>&1)

echo ""
echo "3. Go unit tests"
echo "-----------------"
(cd "${PROJECT_ROOT}/go-scheduler" && GONOSUMCHECK=* GONOSUMDB=* go test ./... -count=1 -v 2>&1)

echo ""
echo "4. HTTP API integration"
echo "-----------------------"

# Start engine
"${PROJECT_ROOT}/target/release/openpipe-engine" &
ENGINE_PID=$!
sleep 2

check "Health endpoint" "curl -sf http://localhost:9090/api/v1/health"
check "Load demo project" "curl -sf -X POST http://localhost:9090/api/v1/project/load \
  -H 'Content-Type: application/json' \
  -d '{\"path\": \"$DEMO_DIR\"}'"

# Save API responses to temp files for Python verification
curl -sf -X POST http://localhost:9090/api/v1/compile \
  -H 'Content-Type: application/json' \
  -d '{}' > "$TMPDIR/compile.json"
pycheck "Compile all models (6)" "
import sys, json
d = json.load(sys.stdin)
assert len(d['models']) == 6, f'Expected 6 models, got {len(d[\"models\"])}'
print(f'OK ({len(d[\"models\"])} models)')
" "$TMPDIR/compile.json"

curl -sf -X POST http://localhost:9090/api/v1/lineage \
  -H 'Content-Type: application/json' \
  -d '{}' > "$TMPDIR/lineage.json"
pycheck "Column-level lineage (6 models)" "
import sys, json
d = json.load(sys.stdin)
assert len(d['models']) == 6, f'Expected 6, got {len(d[\"models\"])}'
print(f'OK ({len(d[\"models\"])} models)')
" "$TMPDIR/lineage.json"

curl -sf -X POST http://localhost:9090/api/v1/lineage/openlineage \
  -H 'Content-Type: application/json' \
  -d '{}' > "$TMPDIR/ol.json"
pycheck "OpenLineage export" "
import sys, json
d = json.load(sys.stdin)
assert len(d['events']) > 0, 'Expected events'
print(f'OK ({len(d[\"events\"])} events)')
" "$TMPDIR/ol.json"

curl -sf -X POST http://localhost:9090/api/v1/tests/compile \
  -H 'Content-Type: application/json' \
  -d '{}' > "$TMPDIR/tests_compile.json"
pycheck "Compile tests (>=7)" "
import sys, json
d = json.load(sys.stdin)
assert len(d['tests']) >= 7, f'Expected >=7 tests, got {len(d[\"tests\"])}'
print(f'OK ({len(d[\"tests\"])} tests)')
" "$TMPDIR/tests_compile.json"

curl -sf -X POST http://localhost:9090/api/v1/tests/suite \
  -H 'Content-Type: application/json' \
  -d '{}' > "$TMPDIR/tests_suite.json"
pycheck "Test suite (>=7 results)" "
import sys, json
d = json.load(sys.stdin)
assert d['summary']['total'] >= 7, f'Expected >=7, got {d[\"summary\"][\"total\"]}'
print(f\"OK ({d['summary']['passed']}/{d['summary']['total']} passed)\")
" "$TMPDIR/tests_suite.json"

curl -sf http://localhost:9090/api/v1/models/types > "$TMPDIR/types.json"
pycheck "Model types (6 models)" "
import sys, json
d = json.load(sys.stdin)
assert len(d['models']) == 6, f'Expected 6, got {len(d[\"models\"])}'
for m in d['models']:
    print(f'  {m[\"name\"]}: {m[\"language\"]} ({m[\"materialization\"]})')
print(f'OK ({len(d[\"models\"])} models)')
" "$TMPDIR/types.json"

echo ""
echo "5. DAG edges verification"
echo "--------------------------"
pycheck "DAG has 11 edges" "
import sys, json
d = json.load(sys.stdin)
edges = d['dag_edges']
assert len(edges) == 11, f'Expected 11 edges, got {len(edges)}'
print(f'OK ({len(edges)} edges)')
" "$TMPDIR/compile.json"

echo ""
echo "=== Results: $PASS passed, $FAIL failed ==="
exit $FAIL
