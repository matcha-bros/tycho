#!/usr/bin/env bash
set -euo pipefail

if [[ -f .env ]]; then
  set -a
  # shellcheck disable=SC1091
  . ./.env
  set +a
fi

required=(
  AUTH_API_KEY
)

for name in "${required[@]}"; do
  if [[ -z "${!name:-}" ]]; then
    echo "missing required env var: $name" >&2
    exit 2
  fi
done

export TYCHO_URL="${TYCHO_URL:-localhost:4242}"
export TYCHO_API_KEY="${TYCHO_API_KEY:-$AUTH_API_KEY}"

run_quote() {
  local title="$1"
  shift

  echo
  echo "== $title =="
  timeout "${QUOTE_TIMEOUT_SECONDS:-120s}" \
    cargo run -q -p tycho-simulation --example local_http_uniswap_v2_quote "$@"
}

SCAN_PROTOCOLS=uniswap_v2,uniswap_v3,sushiswap_v2,pancakeswap_v2,pancakeswap_v3,uniswap_v4 \
SELL_TOKEN=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
BUY_TOKEN=0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 \
SELL_AMOUNT=1 \
run_quote "USDC -> WETH, V2/V3/V4 families"

SCAN_PROTOCOLS=vm:curve \
SELL_TOKEN=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
BUY_TOKEN=0x6B175474E89094C44Da98b954EedeAC495271d0F \
SELL_AMOUNT=1 \
run_quote "USDC -> DAI, Curve VM"

SCAN_PROTOCOLS=vm:balancer_v2 \
SELL_TOKEN=0xba100000625a3754423978a60c9317c58a424e3D \
BUY_TOKEN=0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2 \
SELL_AMOUNT=1 \
run_quote "BAL -> WETH, Balancer V2 VM"

SCAN_PROTOCOLS=ekubo_v2 \
SELL_TOKEN=0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48 \
BUY_TOKEN=0xdAC17F958D2ee523a2206206994597C13D831ec7 \
SELL_AMOUNT=1 \
run_quote "USDC -> USDT, Ekubo V2"
