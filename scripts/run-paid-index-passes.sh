#!/usr/bin/env bash
set -euo pipefail

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

required=(
  DATABASE_URL
  RPC_URL
  SUBSTREAMS_API_TOKEN
)

for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required env var: $name" >&2
    exit 2
  fi
done

endpoint="${SUBSTREAMS_ENDPOINT:-https://eth.substreams.pinax.network:443}"
retention_horizon="${RETENTION_HORIZON:-2020-01-01T00:00:00}"
timeout_seconds="${INDEX_PASS_TIMEOUT_SECONDS:-0}"
indexer="${TYCHO_INDEXER_BIN:-target/debug/tycho-indexer}"

export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
mkdir -p logs

run_pass() {
  local config="$1"
  local label="$2"
  local log_file="logs/${label}-$(date -u +%Y%m%dT%H%M%SZ).log"
  local cmd=(
    "$indexer"
    --database-url "$DATABASE_URL"
    --rpc-url "$RPC_URL"
    --endpoint "$endpoint"
    index
    --substreams-api-token "$SUBSTREAMS_API_TOKEN"
    --extractors-config "$config"
    --retention-horizon "$retention_horizon"
  )

  echo "running $label with $config"
  echo "log: $log_file"

  if [[ "$timeout_seconds" != "0" ]]; then
    set +e
    RUST_LOG="${RUST_LOG:-info}" timeout "$timeout_seconds" "${cmd[@]}" 2>&1 | tee "$log_file"
    local status="${PIPESTATUS[0]}"
    set -e
    if [[ "$status" == "124" ]]; then
      echo "$label reached INDEX_PASS_TIMEOUT_SECONDS=$timeout_seconds; continuing"
      return 0
    fi
    return "$status"
  else
    RUST_LOG="${RUST_LOG:-info}" "${cmd[@]}" 2>&1 | tee "$log_file"
  fi
}

run_pass local-extractors-paid-5a.yaml paid-5a
run_pass local-extractors-paid-5b.yaml paid-5b
