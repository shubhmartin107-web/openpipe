#!/usr/bin/env bash
set -uo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

echo "=== OpenPipe Test Suite ==="
echo ""

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

PASS=0
FAIL=0

check() {
  local name="$1"
  shift
  echo -n "  [ ] $name ... "
  if "$@" > /dev/null 2>&1; then
    echo -e "${GREEN}PASS${NC}"
    PASS=$((PASS + 1))
  else
    echo -e "${RED}FAIL${NC}"
    FAIL=$((FAIL + 1))
  fi
}

export PATH="$HOME/.cargo/bin:$HOME/.local/go/bin:$PATH"

echo "--- Rust Engine ---"
check "cargo check" cargo check --manifest-path rust-engine/Cargo.toml -q
check "cargo test" cargo test --manifest-path rust-engine/Cargo.toml -q

echo ""
echo "--- Go Scheduler ---"
check "go build" go build -C go-scheduler ./cmd/openpipe
check "go vet" go vet -C go-scheduler ./...
check "go test" go test -C go-scheduler ./... -count=1

echo ""

if [ "$FAIL" -eq 0 ]; then
  echo -e "${GREEN}All $PASS tests passed.${NC}"
else
  echo -e "${RED}$FAIL tests failed, $PASS passed.${NC}"
  exit 1
fi
