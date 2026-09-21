#!/usr/bin/env bash
# Arenaton can request any market; PostgreSQL reuses fresh research for 24 hours.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
bash scripts/database.sh start
mise exec rust@stable -- cargo build --locked --features server
exec ./target/debug/polyrover serve \
  --bind 127.0.0.1:8787 \
  --allow-origin http://localhost:8080 \
  --allow-origin http://127.0.0.1:8080 \
  --enable-generation \
  --daily-generation-limit "${POLYROVER_DAILY_GENERATION_LIMIT:-10}" "$@"
