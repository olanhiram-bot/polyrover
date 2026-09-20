#!/usr/bin/env bash
# Local-only PostgreSQL: Unix socket + OS-user authentication, no public TCP port.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
repo="$(pwd -P)"
data="$repo/research/postgres"
socket="$repo/research/postgres-socket"
port=55432
pg() {
  if command -v mise >/dev/null 2>&1; then mise exec postgres@17 -- "$@";
  else "$@"; fi
}
case "${1:-status}" in
  start)
    mkdir -p "$socket"
    chmod 700 "$socket"
    if [[ ! -f "$data/PG_VERSION" ]]; then
      pg initdb -D "$data" --auth-local=peer --auth-host=reject --no-instructions
    fi
    if ! pg pg_ctl -D "$data" status >/dev/null 2>&1; then
      pg pg_ctl -D "$data" -l "$repo/research/postgres.log" \
        -o "-k '$socket' -h '' -p $port" -w start
    fi
    if [[ "$(pg psql -h "$socket" -p "$port" -d postgres -Atc "SELECT 1 FROM pg_database WHERE datname='polyrover'")" != 1 ]]; then
      pg createdb -h "$socket" -p "$port" polyrover
    fi
    echo "Local PostgreSQL ready (Unix socket, peer authentication)."
    ;;
  stop) pg pg_ctl -D "$data" -m fast -w stop ;;
  status) pg pg_ctl -D "$data" status ;;
  *) echo "Usage: bash scripts/database.sh [start|stop|status]" >&2; exit 2 ;;
esac
