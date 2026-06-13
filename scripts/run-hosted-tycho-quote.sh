#!/usr/bin/env bash
set -euo pipefail

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

required=(
  TYCHO_API_KEY
)

for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required env var: $name" >&2
    exit 2
  fi
done

export TYCHO_URL="${TYCHO_URL:-tycho-fynd-ethereum.propellerheads.xyz}"
export TOKEN_MIN_QUALITY="${TOKEN_MIN_QUALITY:-100}"
export MAX_DAYS_SINCE_LAST_TRADE="${MAX_DAYS_SINCE_LAST_TRADE:-3}"
export TVL_GT="${TVL_GT:-10}"
export SELL_AMOUNT="${SELL_AMOUNT:-1}"
export SCAN_PROTOCOLS="${SCAN_PROTOCOLS:-uniswap_v2,uniswap_v3,sushiswap_v2,pancakeswap_v2,pancakeswap_v3,uniswap_v4,ekubo_v2}"

timeout "${QUOTE_TIMEOUT_SECONDS:-60s}" \
  cargo run -q -p tycho-simulation --example local_http_uniswap_v2_quote
