#!/usr/bin/env bash
set -euo pipefail

database_url="${DATABASE_URL:-postgres://postgres:mypassword@localhost:5431/tycho_indexer_0}"
database_name="${database_url##*/}"
database_name="${database_name%%\?*}"

if command -v psql >/dev/null 2>&1; then
  psql_cmd=(psql "$database_url" -v ON_ERROR_STOP=1)
else
  export DOCKER_HOST="${DOCKER_HOST:-unix:///var/run/docker.sock}"
  psql_cmd=(
    docker exec -i -e PGPASSWORD=mypassword docker-db-1
    psql -U postgres -d "$database_name" -v ON_ERROR_STOP=1
  )
fi

"${psql_cmd[@]}" <<'SQL'
\pset pager off

select
  es.name,
  b.number as indexed_block,
  b.ts as indexed_ts,
  coalesce(pc.components, 0) as components
from extraction_state es
join block b on b.id = es.block_id
left join protocol_system ps on ps.name = es.name
left join lateral (
  select count(*) as components
  from protocol_component pc
  where pc.protocol_system_id = ps.id
) pc on true
order by es.name;

select
  inhparent::regclass as parent_table,
  count(*) as partitions
from pg_inherits
where inhparent in (
  'protocol_state'::regclass,
  'component_balance'::regclass,
  'contract_storage'::regclass
)
group by 1
order by 1;
SQL
