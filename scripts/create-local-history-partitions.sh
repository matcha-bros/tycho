#!/usr/bin/env bash
set -euo pipefail

start_date="${1:-2020-04-15}"
end_date="${2:-$(date -u +%F)}"
database_url="${DATABASE_URL:-postgres://postgres:mypassword@localhost:5431/tycho_indexer_0}"
database_name="${database_url##*/}"
database_name="${database_name%%\?*}"

if [[ ! "$start_date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "invalid start date: $start_date" >&2
  exit 2
fi

if [[ ! "$end_date" =~ ^[0-9]{4}-[0-9]{2}-[0-9]{2}$ ]]; then
  echo "invalid end date: $end_date" >&2
  exit 2
fi

if command -v psql >/dev/null 2>&1; then
  psql_cmd=(psql "$database_url" -q -v ON_ERROR_STOP=1)
else
  export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
  psql_cmd=(
    docker exec -i -e PGPASSWORD=mypassword docker-db-1
    psql -U postgres -d "$database_name" -q -v ON_ERROR_STOP=1
  )
fi

if [[ "$(date -u -d "$end_date" +%s)" -lt "$(date -u -d "$start_date" +%s)" ]]; then
  echo "end date $end_date is before start date $start_date" >&2
  exit 2
fi

emit_create_partition() {
  local table="$1"
  local day="$2"
  local next_day="$3"
  local part="${table}_$(date -u -d "$day" +%Y_%m_%d)"

  cat <<SQL
DO \$\$
BEGIN
  BEGIN
    EXECUTE format(
      'CREATE TABLE IF NOT EXISTS %I PARTITION OF %I FOR VALUES FROM (%L) TO (%L)',
      '$part',
      '$table',
      '$day'::timestamptz,
      '$next_day'::timestamptz
    );
  EXCEPTION WHEN OTHERS THEN
    IF SQLERRM NOT LIKE '%would overlap partition%' THEN
      RAISE;
    END IF;
  END;
END \$\$;
SQL
}

{
  current="$start_date"
  while [[ "$(date -u -d "$current" +%s)" -le "$(date -u -d "$end_date" +%s)" ]]; do
    next="$(date -u -d "$current + 1 day" +%F)"
    emit_create_partition component_balance "$current" "$next"
    emit_create_partition contract_storage "$current" "$next"
    emit_create_partition protocol_state "$current" "$next"
    current="$next"
  done
} | "${psql_cmd[@]}"
