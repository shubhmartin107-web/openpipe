#!/usr/bin/env bash
# Returns current time in milliseconds
if command -v gdate &>/dev/null; then
  gdate +%s%3N
else
  date +%s%3N
fi
