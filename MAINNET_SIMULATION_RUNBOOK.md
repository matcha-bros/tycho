# Tycho Mainnet Simulation Runbook

This worktree now has two usable paths:

- Hosted current-state quoting through the Tycho Fynd API. This is the fastest
  MVP path for router experiments against fresh Uniswap/Sushi/Pancake/Ekubo
  liquidity.
- Self-hosted indexing through local Postgres plus local `.spkg` Substreams
  packages executed on a remote Substreams provider. This path uses no Tycho
  hosted API key, but historical backfill is the slow part.

## What Runs Locally

- Local Postgres: `docker-db-1`, exposed on `localhost:5431`.
- Local Tycho binaries: `target/debug/tycho-indexer`, `target/debug/tycho-client`.
- Local Substreams packages: generated under
  `protocols/substreams/ethereum-uniswap-v2/*.local.spkg`.
- Local extractor config: `local-extractors.yaml`.

The remote dependency is the Substreams endpoint. We are not running a local
Substreams node or archive node.

## Required Environment

The checked-in `.gitignore` already ignores `.env` and `docker/.env`.

Hosted current-state quoting:

```sh
TYCHO_URL=tycho-fynd-ethereum.propellerheads.xyz
TYCHO_API_KEY=<tycho-hosted-api-key>
TOKEN_MIN_QUALITY=100
MAX_DAYS_SINCE_LAST_TRADE=3
TVL_GT=10
```

Self-hosted indexing:

```sh
SUBSTREAMS_API_TOKEN="Bearer <substreams-jwt>"
AUTH_API_KEY=local-dev-key
DATABASE_URL=postgres://postgres:mypassword@localhost:5431/tycho_indexer_0
RPC_URL=<ethereum-rpc-url>
```

Recommended for VM/DCI protocols:

```sh
TRACE_RPC_URL=<ethereum-trace-capable-rpc-url>
```

The current paid Substreams plan was observed to allow 5 concurrent streams and
15 workers. Tycho opens one Substreams stream per extractor, so a 5-stream plan
can run the nine configured Ethereum extractors in two passes: 5 + 4. Running
all nine at once still needs at least 9 active streams.

For the `substreams` CLI only, strip the `Bearer ` prefix before passing the
token:

```sh
raw_token=${SUBSTREAMS_API_TOKEN#Bearer }
```

For `tycho-indexer`, keep the `Bearer ` prefix because it sends the value
verbatim as the authorization metadata.

## Hosted Current-State Quote

This is the fastest reproducible command from this worktree:

```sh
set -a; . ./.env; set +a
./scripts/run-hosted-tycho-quote.sh
```

It loads the hosted Tycho API key from `.env`, uses the Fynd endpoint, applies
the plan-required token and TVL filters, scans the allowed protocols for the
requested token pair, and runs Tycho's protocol simulators locally.

Default pair and amount:

```sh
SELL_TOKEN=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48  # USDC
BUY_TOKEN=0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2   # WETH
SELL_AMOUNT=1
```

Observed output on 2026-06-13:

```text
ekubo_v2: no indexed pool for requested token pair
Best quotes for 1.000000000000 USDC -> WETH:
uniswap_v3       0xe0554a476a092703abdb3ef35c80e0d76d32939f created_at=2021-11-14 21:44:29 out=0.000597121764 WETH raw=597121763658033 gas=147500
uniswap_v3       0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640 created_at=2021-05-05 21:42:11 out=0.000596775726 WETH raw=596775726374670 gas=147500
pancakeswap_v3   0x1445f32d1a74872ba41f3d8cf4022e9996120b31 created_at=2024-03-13 07:13:23 out=0.000596673807 WETH raw=596673806691515 gas=147500
uniswap_v4       0x5a5c7cab5f55c7ea020e97d4fa6dd5d99270e56ce76afa61d8cbddec0af92060 created_at=2026-01-26 13:04:47 out=0.000596588449 WETH raw=596588448987360 gas=218500
pancakeswap_v3   0x1ac1a8feaaea1900c4166deeed0c11cc10669d36 created_at=2023-04-01 14:37:35 out=0.000596377841 WETH raw=596377840629306 gas=147500
uniswap_v4       0x4f88f7c99022eace4740c6898f59ce6a2e798a1e64ce54589720b7153eb224a7 created_at=2025-01-24 17:48:23 out=0.000596338149 WETH raw=596338149315931 gas=218500
uniswap_v2       0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc created_at=2020-05-05 20:22:25 out=0.000595976620 WETH raw=595976620329300 gas=90000
sushiswap_v2     0x397ff1542f962076d0bfe58ea045ffa2d347aca0 created_at=2020-09-09 19:06:45 out=0.000595736112 WETH raw=595736112009335 gas=90000
pancakeswap_v2   0x2e8135be71230c6b1b4045696d41c09db0414226 created_at=2022-09-27 08:04:11 out=0.000594520024 WETH raw=594520023594501 gas=90000
```

The current hosted plan allows:

- `uniswap_v2`
- `sushiswap_v2`
- `pancakeswap_v2`
- `uniswap_v3`
- `pancakeswap_v3`
- `uniswap_v4`
- `ekubo_v2`
- `ekubo_v3`
- `fluid_v1`

The current hosted plan does not allow `vm:curve` or `vm:balancer_v2`; the API
returns `protocol_system ... is not available on this plan` for those names.
That is acceptable for an initial router MVP over Uniswap-style liquidity, but
it is not enough for a serious broad comparison against 1inch because stable,
LST, Balancer, and Curve liquidity can dominate many routes. Ask Tycho/Propeller
Heads for Curve/Balancer/full protocol access if the benchmark is "beat 1inch"
rather than "prototype the routing engine against live liquidity."

Dune analysis in `../dune-analysis/reports` strengthens that recommendation:

- The selected Tycho target protocols cover about 79.43% of analyzed 30d
  target-token venue volume.
- The current hosted-allowed target subset, excluding Curve/Balancer, covers
  about 69.71% of all analyzed venue volume.
- Curve alone is about 15.73% of all analyzed venue volume and about 18.25% of
  the target-protocol volume set.
- Fluid appears as the largest non-target onchain venue bucket in the report,
  about 12.35% of all analyzed venue volume. The hosted plan reports `fluid_v1`
  as allowed and this repo has Fluid simulation code, but this worktree's simple
  scanner does not yet wire `fluid_v1` into `run-hosted-tycho-quote.sh`.

Practical call: current hosted access is enough to start route-search
development over live liquidity. For a credible "beat 1inch" comparison, get
Curve/Balancer/full unlock and wire Fluid into the scanner.

For the current MVP benchmark strategy, see
`../dune-analysis/docs/pair_protocol_coverage.md`. The key benchmark goal is to
choose pairs where a significant share of observed volume comes from protocols
we can simulate with Tycho, then add our custom protocol from our own Tycho
instance.

## Direct-Swap Search API

For a quote-style API that searches direct pools and returns the best simulated
route, start:

```sh
set -a; . ./.env; set +a

DIRECT_SWAP_API_BIND=127.0.0.1:8099 \
TYCHO_URL=tycho-fynd-ethereum.propellerheads.xyz \
TOKEN_MIN_QUALITY=100 \
MAX_DAYS_SINCE_LAST_TRADE=3 \
TVL_GT=10 \
SCAN_PROTOCOLS=uniswap_v2,uniswap_v3,sushiswap_v2,pancakeswap_v2,pancakeswap_v3,uniswap_v4,ekubo_v2 \
cargo run -q -p tycho-simulation --example direct_swap_search_api
```

Quote `1 USDC -> WETH`:

```sh
curl -sS -X POST http://127.0.0.1:8099/quote/direct \
  -H 'Content-Type: application/json' \
  --data '{
    "sellToken":"0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
    "buyToken":"0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    "amountIn":"1000000",
    "minAmountOut":"1",
    "blockNumber":25308649
  }' | jq .
```

The API uses raw integer amounts. It treats native ETH sentinel addresses
(`0x000...000` and `0xEeee...EEeE`) as WETH for pool search and marks
`wrapOrUnwrapRequired=true`. `blockNumber` is optional and pins protocol state
requests to a specific Ethereum block.

Pinned blocks are best-effort with hosted Tycho access and should be treated as
unstable in this MVP. The hosted Fynd plan tested here rejects stale historical
blocks with `Snapshot block is stale: version is older than the 10 minute limit
on this plan`. On 2026-06-13, pinned quotes worked for recent blocks but failed
around 50 Ethereum blocks behind the current head. A one-year-back block also
failed with the same stale-snapshot error. Use this parameter for short-window
debug reproducibility only unless the hosted plan is upgraded or the API is
pointed at our own historical Tycho indexer.

Live, unpinned requests use the latest state served by Tycho, but this example
cannot report exact server-side data lag because `get_protocol_states` responses
do not include the indexed block height. The practical freshness signal we can
observe from this API path is that recent pinned blocks, roughly within the
hosted plan's 10-minute retention window, succeed while older pinned blocks are
rejected.

To check live quote freshness, fetch the latest Ethereum block from an RPC you
trust, then pass that exact value as `blockNumber`. If it succeeds, the quote
path can serve that block. If it fails as missing or stale, count down from that
block until the first success; the difference is the observed lag in blocks for
this API path at that moment. On 2026-06-13, a pinned request for the public RPC
latest block `25308743` succeeded, so the observed lag was `0` blocks in that
test.

MVP limitations:

- Direct pair only.
- Full `amountIn` into every candidate pool.
- No multi-hop search.
- No split routing.
- No gas-adjusted ranking.
- No calldata generation.

## Start Postgres

```sh
DOCKER_HOST=unix:///var/run/docker.sock \
TYCHO_IMAGE=tycho-indexer-placeholder \
docker compose -f docker/docker-compose.yaml up -d db
```

This uses the repo's Postgres image (`docker-db`, based on
`ghcr.io/dbsystel/postgresql-partman`) and installs `pg_cron`. It is not a
Substreams node.

For historical mainnet backfills with thousands of daily partitions, raise the
local Postgres lock budget once and restart the DB:

```sh
DOCKER_HOST=unix:///var/run/docker.sock docker exec -e PGPASSWORD=mypassword docker-db-1 \
  psql -U postgres -d postgres \
  -c "alter system set max_locks_per_transaction = 1024;"

DOCKER_HOST=unix:///var/run/docker.sock docker restart docker-db-1
```

For a clean smoke DB:

```sh
DOCKER_HOST=unix:///var/run/docker.sock docker exec -e PGPASSWORD=mypassword docker-db-1 \
  psql -U postgres -d postgres \
  -c "drop database if exists tycho_indexer_0 with (force);" \
  -c "create database tycho_indexer_0;"
```

## Build Local Binaries

```sh
mkdir -p .local/lib
ln -sf /lib/x86_64-linux-gnu/libpq.so.5 .local/lib/libpq.so
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

cargo build --bin tycho-indexer
cargo build --bin tycho-client
cargo check -p tycho-simulation --example quickstart
cargo check -p tycho-simulation --example local_http_uniswap_v2_quote
```

The `LIBRARY_PATH` workaround is needed on this machine because the unversioned
`libpq.so` linker name is missing.

## Verify Substreams Auth And Local `.spkg`

```sh
set -a; . ./.env; set +a
export PATH="$PWD/.local/bin:$PATH"
raw_token=${SUBSTREAMS_API_TOKEN#Bearer }

SUBSTREAMS_API_TOKEN="$raw_token" substreams run \
  -e eth.substreams.pinax.network:443 \
  protocols/substreams/ethereum-uniswap-v2/ethereum-uniswap-v2.local.spkg \
  map_pool_events \
  -s 10008300 \
  -t +1
```

This succeeded locally and returned Tycho `BlockChanges` for Ethereum block
`10008300`, proving the The Graph/Pinax Substreams JWT can execute our local
package remotely.

## Working Local Indexer Smoke Test

Apply migrations by briefly starting RPC mode:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

timeout 5s target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  rpc || true
```

For historical indexing, create historical daily partitions before indexing.
Without these, old rows land in default partitions and can hit duplicate-key
conflicts or excessive partition lock pressure. This must include
`protocol_state`, `component_balance`, and `contract_storage`; VM protocols
like Curve and Balancer need `contract_storage`. The helper below falls back to
`docker exec docker-db-1 psql` if local `psql` is not installed:

```sh
set -a; . ./.env; set +a
./scripts/create-local-history-partitions.sh 2020-04-15 2026-06-13
```

Check local progress at any time:

```sh
set -a; . ./.env; set +a
./scripts/local-index-status.sh
```

Start a one-extractor local indexer:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

RUST_LOG=info target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors-smoke-uniswap-v2.yaml \
  --retention-horizon 2020-01-01T00:00:00
```

This produced local DB rows for a real Ethereum Uniswap V2 pool:

- pool: `0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc`
- tokens: `USDC` / `WETH`
- state: `reserve0`, `reserve1`, and component balances

## Serve And Read The Local DB

The `index` command above also serves the local Tycho HTTP API on
`localhost:4242`. If the indexer is not running, start RPC-only mode:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  rpc
```

In another shell:

```sh
set -a; . ./.env; set +a

curl -sS -H "Authorization: $AUTH_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"chain":"ethereum","protocol_system":"uniswap_v2","pagination":{"page":0,"page_size":20}}' \
  http://localhost:4242/v1/protocol_components | jq .

curl -sS -H "Authorization: $AUTH_API_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"chain":"ethereum","protocol_system":"uniswap_v2","protocol_ids":["0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc"],"include_balances":true,"pagination":{"page":0,"page_size":20}}' \
  http://localhost:4242/v1/protocol_state | jq .
```

The second command returned reserves and balances from local Postgres.

## Working Local Simulation

With the smoke indexer running and serving `localhost:4242`, run:

```sh
set -a; . ./.env; set +a

TYCHO_URL=localhost:4242 \
TYCHO_API_KEY="$AUTH_API_KEY" \
SELL_AMOUNT=1 \
timeout 30s cargo run -q -p tycho-simulation --example local_http_uniswap_v2_quote
```

Observed output:

```text
Protocol: uniswap_v2
Pool: 0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc
Swap: 1.000000000000 USDC -> 0.004937413848 WETH
Raw amount out: 4937413847642776
Gas: 90000
```

This is a real mainnet protocol simulation result from local Tycho-indexed
state. It uses no Tycho hosted API key. The only external services are the
Ethereum RPC endpoint and the Substreams provider executing the local `.spkg`.

### Quote Coverage

Current proven local quote coverage:

- Direct exact-pool quote for `uniswap_v2`.
- Token-pair scan and quote across V2-style, V3-style, V4, Ekubo, and VM pools:
  `uniswap_v2`, `sushiswap_v2`, `pancakeswap_v2`, `uniswap_v3`,
  `pancakeswap_v3`, `uniswap_v4`, `ekubo_v2`, `vm:curve`,
  `vm:balancer_v2`.

The current partial local DB does not yet contain one shared token pair across
all nine protocols. USDC/WETH is available for the V2/V3/V4 families, but the
currently indexed Curve, Balancer, and Ekubo pools use different pairs. Continue
the paid indexing passes to discover deeper liquidity and more common pairs.

The common-pair command scans USDC/WETH across all nine configured names and
quotes the protocols that currently have that pair:

```sh
set -a; . ./.env; set +a

TYCHO_URL=localhost:4242 \
TYCHO_API_KEY="$AUTH_API_KEY" \
SCAN_PROTOCOLS=uniswap_v2,uniswap_v3,sushiswap_v2,vm:balancer_v2,pancakeswap_v2,pancakeswap_v3,vm:curve,uniswap_v4,ekubo_v2 \
SELL_AMOUNT=1 \
timeout 120s cargo run -q -p tycho-simulation --example local_http_uniswap_v2_quote
```

Observed output against the local DB:

```text
vm:balancer_v2: no indexed pool for requested token pair
vm:curve: no indexed pool for requested token pair
ekubo_v2: no indexed pool for requested token pair
Best local quotes for 1.000000000000 USDC -> WETH:
uniswap_v2       0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc created_at=2020-05-05 20:22:25 out=0.004200815595 WETH raw=4200815594768370 gas=90000
sushiswap_v2     0x397ff1542f962076d0bfe58ea045ffa2d347aca0 created_at=2020-09-09 19:06:45 out=0.002588297358 WETH raw=2588297357990657 gas=90000
pancakeswap_v2   0x2e8135be71230c6b1b4045696d41c09db0414226 created_at=2022-09-27 08:04:11 out=0.000739932135 WETH raw=739932134509935 gas=90000
pancakeswap_v3   0x1ac1a8feaaea1900c4166deeed0c11cc10669d36 created_at=2023-04-01 14:37:35 out=0.000557390871 WETH raw=557390870778066 gas=147500
uniswap_v4       0x4f88f7c99022eace4740c6898f59ce6a2e798a1e64ce54589720b7153eb224a7 created_at=2025-01-24 17:48:23 out=0.000293668196 WETH raw=293668195607320 gas=218500
uniswap_v3       0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640 created_at=2021-05-05 21:42:11 out=0.000283044163 WETH raw=283044162842538 gas=147500
uniswap_v3       0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8 created_at=2021-05-04 23:10:00 out=0.000282379354 WETH raw=282379353852529 gas=147500
uniswap_v3       0x7bea39867e4169dbe237d55c8242a8f2fcdcc387 created_at=2021-05-04 20:07:45 out=0.000279613130 WETH raw=279613129798390 gas=147500
```

To prove every configured protocol family from the current partial DB, run the
coverage script. It uses locally indexed pairs per family:

```sh
set -a; . ./.env; set +a
TYCHO_URL=localhost:4242 ./scripts/run-local-protocol-quotes.sh
```

Observed output:

```text
== USDC -> WETH, V2/V3/V4 families ==
Best local quotes for 1.000000000000 USDC -> WETH:
uniswap_v2       0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc created_at=2020-05-05 20:22:25 out=0.004200815595 WETH raw=4200815594768370 gas=90000
sushiswap_v2     0x397ff1542f962076d0bfe58ea045ffa2d347aca0 created_at=2020-09-09 19:06:45 out=0.002588297358 WETH raw=2588297357990657 gas=90000
pancakeswap_v2   0x2e8135be71230c6b1b4045696d41c09db0414226 created_at=2022-09-27 08:04:11 out=0.000739932135 WETH raw=739932134509935 gas=90000
pancakeswap_v3   0x1ac1a8feaaea1900c4166deeed0c11cc10669d36 created_at=2023-04-01 14:37:35 out=0.000557390871 WETH raw=557390870778066 gas=147500
uniswap_v4       0x4f88f7c99022eace4740c6898f59ce6a2e798a1e64ce54589720b7153eb224a7 created_at=2025-01-24 17:48:23 out=0.000293668196 WETH raw=293668195607320 gas=218500
uniswap_v3       0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640 created_at=2021-05-05 21:42:11 out=0.000283044163 WETH raw=283044162842538 gas=147500
uniswap_v3       0x8ad599c3a0ff1de082011efddc58f1908eb6e6d8 created_at=2021-05-04 23:10:00 out=0.000282379354 WETH raw=282379353852529 gas=147500
uniswap_v3       0x7bea39867e4169dbe237d55c8242a8f2fcdcc387 created_at=2021-05-04 20:07:45 out=0.000279613130 WETH raw=279613129798390 gas=147500

== USDC -> DAI, Curve VM ==
Best local quotes for 1.000000000000 USDC -> DAI:
vm:curve         0xa5407eae9ba41422680e2e00537571bcc53efbfd created_at=2020-04-20 02:14:04 out=0.986662070114 DAI raw=986662070114478068 gas=315467

== BAL -> WETH, Balancer V2 VM ==
Best local quotes for 1.000000000000 BAL -> WETH:
vm:balancer_v2   0x647c1fd457b95b75d0972ff08fe01d7d7bda05df000200000000000000000002 created_at=2021-04-21 22:27:31 out=0.020981897407 WETH raw=20981897407240393 gas=118712

== USDC -> USDT, Ekubo V2 ==
Best local quotes for 1.000000000000 USDC -> USDT:
ekubo_v2         0x0e647f6d174aa84c22fddeef0af92262b878ba6f86094e54dbec558c0a53ab79 created_at=2025-03-17 21:47:23 out=0.999996000000 USDT raw=999996 gas=90000
ekubo_v2         0x91ffc128bf8e0afbd2c0f14722e2fd5b6625341a5e5f551aa36242d98756798d created_at=2025-03-14 23:47:35 out=0.999957000000 USDT raw=999957 gas=90000
```

## Full All-Protocol Indexing

The all-protocol config is `local-extractors.yaml` and includes:

- `uniswap_v2`
- `uniswap_v3`
- `sushiswap_v2`
- `vm:balancer_v2`
- `pancakeswap_v2`
- `pancakeswap_v3`
- `vm:curve`
- `uniswap_v4`
- `ekubo_v2`

If the Substreams plan has at least 9 active streams, run all extractors with:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

RUST_LOG=info target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors.yaml \
  --retention-horizon 2024-01-01T00:00:00
```

With the old free key, the provider rejected the all-extractor run with
`Concurrent stream limit exceeded (active sessions: 2/2)`. Tycho starts one
Substreams session per extractor in the config and there is no
`index --concurrency` flag.

To keep all nine protocols live at head simultaneously, the Substreams account
needs at least nine active streams plus enough historical processed-block quota.
To catch up in batches on the free plan, run at most two extractors at a time
with these configs:

```text
local-extractors-batch-1.yaml  # uniswap_v2, sushiswap_v2
local-extractors-batch-2.yaml  # uniswap_v3, vm:balancer_v2
local-extractors-batch-3.yaml  # pancakeswap_v2, pancakeswap_v3
local-extractors-batch-4.yaml  # vm:curve, uniswap_v4
local-extractors-batch-5.yaml  # ekubo_v2
```

Historical catch-up also needs partition coverage for the relevant historical
date ranges, not only the 2020 smoke window above.

With the current paid 5-stream / 15-worker plan, use:

```text
local-extractors-paid-5a.yaml  # uniswap_v2, uniswap_v3, sushiswap_v2, pancakeswap_v2, pancakeswap_v3
local-extractors-paid-5b.yaml  # vm:balancer_v2, vm:curve, uniswap_v4, ekubo_v2
```

Run pass A:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

RUST_LOG=info target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors-paid-5a.yaml \
  --retention-horizon 2020-01-01T00:00:00
```

Run pass B:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

RUST_LOG=info target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors-paid-5b.yaml \
  --retention-horizon 2020-01-01T00:00:00
```

Both paid-plan configs were verified locally with `timeout 180s`. They
initialized all streams without concurrency errors, and Substreams reported
`max_parallel_workers=15` for the upgraded sessions.

The script form runs both passes sequentially and writes logs under `logs/`:

```sh
set -a; . ./.env; set +a
./scripts/run-paid-index-passes.sh
```

For a bounded smoke run:

```sh
set -a; . ./.env; set +a
INDEX_PASS_TIMEOUT_SECONDS=300 ./scripts/run-paid-index-passes.sh
```

Current status after bounded paid-plan runs can be checked with:

```sh
set -a; . ./.env; set +a
./scripts/local-index-status.sh
```

As of the latest local run, all nine extractors had cursor progress and all
nine had emitted component rows. `vm:balancer_v2` began emitting pools after a
bounded paid-plan 5b pass advanced past block `12285749`.

## Local Changes Made

Tracked code changes:

- `crates/tycho-storage/src/postgres/protocol.rs`
  - Deduplicates latest component balances before upserting current state.
  - Avoids archiving `MAX_TS` current balances into history.
  - This was needed for historical local indexing to get past duplicate current
    balance rows.
- `crates/tycho-simulation/examples/quickstart/main.rs`
  - Adds `TYCHO_NO_TLS=1` for local HTTP/WS.
  - Adds exchange/component filters and token quality options for local partial
    DB runs.

New local artifacts:

- `crates/tycho-simulation/examples/local_http_uniswap_v2_quote/`
  - Direct local HTTP quote example.
  - Can quote one exact V2 pool or scan V2-style, V3-style, V4, Ekubo, and VM
    protocols for a token pair.
- `local-extractors*.yaml`
  - All-protocol, free-plan batch, paid-plan batch, and smoke extractor configs.
- `scripts/create-local-history-partitions.sh`
  - Creates historical partitions for `protocol_state`, `component_balance`,
    and `contract_storage`.
- `scripts/local-index-status.sh`
  - Reports local cursor/component progress.
- `scripts/run-paid-index-passes.sh`
  - Runs the paid-plan 5-stream and 4-stream passes sequentially.
- `scripts/run-local-protocol-quotes.sh`
  - Runs the currently verified local quote set covering all configured
    protocol families.

Batch 1 was verified locally with the free Substreams key:

```sh
set -a; . ./.env; set +a
export LIBRARY_PATH="$PWD/.local/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

RUST_LOG=info timeout 180s target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors-batch-1.yaml \
  --retention-horizon 2020-01-01T00:00:00
```

It exited because of the `timeout`, not because of an indexer error. Verified
local DB state afterward:

```sql
select ps.name, min(pc.created_at), max(pc.created_at), count(*)
from protocol_component pc
join protocol_system ps on ps.id = pc.protocol_system_id
group by 1
order by 1;

select es.name, b.number, b.ts
from extraction_state es
join block b on b.id = es.block_id
order by es.name;
```

Observed:

```text
name          components  indexed through
sushiswap_v2 79          block 10848951, 2020-09-12 19:08:20 UTC
uniswap_v2   346         block 10130835, 2020-05-24 20:48:44 UTC
```

The log reported estimated remaining catch-up time of roughly 24-32 hours for
these two older protocols on this machine/provider session. Newer protocols
start much later and should have less historical range to process.

Batch 2 also started successfully:

```sh
RUST_LOG=info timeout 180s target/debug/tycho-indexer \
  --database-url "$DATABASE_URL" \
  --rpc-url "$RPC_URL" \
  --endpoint https://eth.substreams.pinax.network:443 \
  index \
  --substreams-api-token "$SUBSTREAMS_API_TOKEN" \
  --extractors-config local-extractors-batch-2.yaml \
  --retention-horizon 2020-01-01T00:00:00
```

Observed progress after timeout:

```text
name            components  indexed through
uniswap_v3      24          block 12374914, 2021-05-05 15:04:19 UTC
vm:balancer_v2  0           block 12278140, 2021-04-20 16:36:25 UTC
```

Balancer V2 was processing VM contract state but had not yet emitted pool
components in the short run. Token metadata `eth_call` reverts appeared as
warnings and did not stop the indexer.

After running all batches/passes, the local DB had cursor progress for all nine
configured extractors:

```text
name            indexed through
ekubo_v2        block 22062234, 2025-03-16 21:49:35 UTC
pancakeswap_v2  block 15663936, 2022-10-03 00:06:11 UTC
pancakeswap_v3  block 16967476, 2023-04-03 08:59:47 UTC
sushiswap_v2    block 10869988, 2020-09-16 00:43:27 UTC
uniswap_v2      block 10156801, 2020-05-28 22:07:34 UTC
uniswap_v3      block 12382958, 2021-05-06 20:38:33 UTC
uniswap_v4      block 21700236, 2025-01-25 08:10:11 UTC
vm:balancer_v2  block 12285140, 2021-04-21 18:27:52 UTC
vm:curve        block 9921398, 2020-04-22 09:12:44 UTC
```

Component rows were present for all except Balancer V2 in the short run:

```text
ekubo_v2        10
pancakeswap_v2  94
pancakeswap_v3  10
sushiswap_v2    100
uniswap_v2      422
uniswap_v3      516
uniswap_v4      10
vm:curve        1
```

## WebSocket Quickstart Caveat

The quickstart example now supports local non-TLS Tycho by setting
`TYCHO_NO_TLS=1`.

Start the full `index` command above and leave it running. In another shell,
after the DB has current USDC/WETH liquidity:

```sh
set -a; . ./.env; set +a

TYCHO_URL=localhost:4242 \
TYCHO_API_KEY="$AUTH_API_KEY" \
TYCHO_NO_TLS=1 \
cargo run -p tycho-simulation --example quickstart -- \
  --chain ethereum \
  --sell-token 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
  --buy-token 0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 \
  --sell-amount 10 \
  --tvl-threshold 1000
```

That command uses the local Tycho API and local indexed state. It does not need
`TYCHO_API_KEY` from PropellerHeads; `TYCHO_API_KEY` is only the local
`AUTH_API_KEY` expected by the self-hosted server.

For the historical smoke DB, the WebSocket quickstart compiled and connected but
failed during fast catch-up with missing snapshot block rows. The direct HTTP
quote example above is the currently reproducible local simulation command.

## Custom Protocol Path

For a new protocol, add a new package under `protocols/substreams`, pack it into
`.spkg`, add it to the extractor config, and run the local indexer against the
same remote Substreams endpoint. You do not need PropellerHeads to index your
protocol, but the remote Substreams provider must be able to execute your
package and your plan must cover the historical processing/concurrent stream
load.
