use std::{collections::HashMap, env, net::SocketAddr, str::FromStr};

use alloy::primitives::U256;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tycho_simulation::{
    evm::protocol::{
        ekubo::state::EkuboState, uniswap_v2::state::UniswapV2State,
        uniswap_v3::state::UniswapV3State, uniswap_v4::state::UniswapV4State,
    },
    protocol::models::{DecoderContext, TryFromWithBlock},
    tycho_client::{
        feed::{synchronizer::ComponentWithState, BlockHeader},
        rpc::{
            HttpRPCClientOptions, ProtocolComponentsPaginatedParams, ProtocolStatesParams,
            RPCClient, TokensParams,
        },
        HttpRPCClient,
    },
    tycho_common::{
        models::{protocol::ProtocolComponent, token::Token, Chain},
        simulation::protocol_sim::ProtocolSim,
        Bytes,
    },
};

const DEFAULT_TYCHO_URL: &str = "tycho-fynd-ethereum.propellerheads.xyz";
const DEFAULT_BIND: &str = "127.0.0.1:8088";
const WETH_ADDRESS: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
const ZERO_ADDRESS: &str = "0x0000000000000000000000000000000000000000";
const NATIVE_ETH_ADDRESS: &str = "0xEeeeeEeeeEeEeeEeEeEeeEEEeeeeEeeeeeeeEEeE";

const DEFAULT_PROTOCOLS: &[&str] = &[
    "uniswap_v2",
    "uniswap_v3",
    "sushiswap_v2",
    "pancakeswap_v2",
    "pancakeswap_v3",
    "uniswap_v4",
    "ekubo_v2",
];

#[derive(Clone)]
struct AppState {
    client: HttpRPCClient,
    protocols: Vec<String>,
    token_min_quality: i32,
    max_days_since_last_trade: u64,
    tvl_gt: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectQuoteRequest {
    sell_token: String,
    buy_token: String,
    amount_in: String,
    min_amount_out: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectQuoteResponse {
    sell_token: String,
    buy_token: String,
    normalized_sell_token: String,
    normalized_buy_token: String,
    wrap_or_unwrap_required: bool,
    amount_in: String,
    min_amount_out: String,
    best_quote: Option<QuoteCandidate>,
    meets_min_amount: bool,
    candidate_count: usize,
    successful_quote_count: usize,
    skipped_count: usize,
    candidates: Vec<QuoteCandidate>,
    skipped_protocols: Vec<SkippedProtocol>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct QuoteCandidate {
    protocol: String,
    pool: String,
    created_at: String,
    amount_out: String,
    formatted_amount_out: String,
    gas: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SkippedProtocol {
    protocol: String,
    reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorResponse {
    error: String,
}

#[derive(Clone, Copy)]
enum ProtocolKind {
    V2,
    V3,
    V4,
    Ekubo,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv::dotenv().ok();
    tracing_subscriber::fmt::init();

    let tycho_url = env::var("TYCHO_URL").unwrap_or_else(|_| DEFAULT_TYCHO_URL.to_string());
    let auth_key = env::var("TYCHO_API_KEY")
        .or_else(|_| env::var("AUTH_API_KEY"))
        .ok();
    let bind: SocketAddr = env::var("DIRECT_SWAP_API_BIND")
        .unwrap_or_else(|_| DEFAULT_BIND.to_string())
        .parse()?;

    let protocols = env::var("SCAN_PROTOCOLS")
        .ok()
        .map(|value| parse_protocols(&value))
        .filter(|protocols| !protocols.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_PROTOCOLS
                .iter()
                .map(|value| value.to_string())
                .collect()
        });

    let state = AppState {
        client: HttpRPCClient::new(
            &format!("http://{tycho_url}"),
            HttpRPCClientOptions::new()
                .with_auth_key(auth_key)
                .with_compression(true),
        )?,
        protocols,
        token_min_quality: env::var("TOKEN_MIN_QUALITY")
            .unwrap_or_else(|_| "100".to_string())
            .parse()?,
        max_days_since_last_trade: env::var("MAX_DAYS_SINCE_LAST_TRADE")
            .unwrap_or_else(|_| "3".to_string())
            .parse()?,
        tvl_gt: env::var("TVL_GT")
            .unwrap_or_else(|_| "10".to_string())
            .parse()?,
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/quote/direct", post(quote_direct))
        .with_state(state);

    let listener = TcpListener::bind(bind).await?;
    println!("direct swap search API listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn quote_direct(
    State(state): State<AppState>,
    Json(request): Json<DirectQuoteRequest>,
) -> Result<Json<DirectQuoteResponse>, (StatusCode, Json<ErrorResponse>)> {
    search_direct_quote(&state, request)
        .await
        .map(Json)
        .map_err(internal_error)
}

async fn search_direct_quote(
    state: &AppState,
    request: DirectQuoteRequest,
) -> anyhow::Result<DirectQuoteResponse> {
    let requested_sell_token = parse_address(&request.sell_token, "sellToken")?;
    let requested_buy_token = parse_address(&request.buy_token, "buyToken")?;
    let normalized_sell_token = normalize_native_token(&requested_sell_token)?;
    let normalized_buy_token = normalize_native_token(&requested_buy_token)?;
    let wrap_or_unwrap_required = normalized_sell_token != requested_sell_token
        || normalized_buy_token != requested_buy_token;
    let amount_in = parse_biguint(&request.amount_in, "amountIn")?;
    let min_amount_out = parse_biguint(&request.min_amount_out, "minAmountOut")?;

    let tokens = state
        .client
        .get_tokens(
            TokensParams::new(Chain::Ethereum)
                .with_min_quality(state.token_min_quality)
                .with_traded_n_days_ago(state.max_days_since_last_trade),
        )
        .await?
        .into_data()
        .into_iter()
        .map(|token| (token.address.clone(), token))
        .collect::<HashMap<_, _>>();

    let sell_token = tokens
        .get(&normalized_sell_token)
        .ok_or_else(|| anyhow::anyhow!("sell token not found in Tycho token list"))?;
    let buy_token = tokens
        .get(&normalized_buy_token)
        .ok_or_else(|| anyhow::anyhow!("buy token not found in Tycho token list"))?;

    let mut candidates = Vec::new();
    let mut skipped_protocols = Vec::new();
    let mut candidate_count = 0usize;

    for protocol in &state.protocols {
        let Some(protocol_kind) = protocol_kind(protocol) else {
            skipped_protocols.push(SkippedProtocol {
                protocol: protocol.clone(),
                reason: "unsupported protocol family in direct search MVP".to_string(),
            });
            continue;
        };

        let components = state
            .client
            .get_protocol_components_paginated(
                ProtocolComponentsPaginatedParams::new(Chain::Ethereum, protocol, 4)
                    .with_chunk_size(500)
                    .with_tvl_gt(state.tvl_gt),
            )
            .await?;

        let pair_components = components
            .into_iter()
            .filter(|component| {
                has_token_pair(component, &normalized_sell_token, &normalized_buy_token)
            })
            .collect::<Vec<_>>();

        if pair_components.is_empty() {
            skipped_protocols.push(SkippedProtocol {
                protocol: protocol.clone(),
                reason: "no indexed pool for requested token pair".to_string(),
            });
            continue;
        }
        candidate_count += pair_components.len();

        let states = state
            .client
            .get_protocol_states(
                ProtocolStatesParams::new(Chain::Ethereum, protocol)
                    .with_protocol_ids(
                        pair_components
                            .iter()
                            .map(|component| component.id.clone())
                            .collect(),
                    )
                    .with_include_balances(true),
            )
            .await?
            .into_data()
            .into_iter()
            .map(|state| (state.component_id.clone(), state))
            .collect::<HashMap<_, _>>();

        let mut protocol_successes = 0usize;
        for component in pair_components {
            let Some(component_state) = states.get(&component.id) else {
                continue;
            };
            let snapshot = ComponentWithState {
                state: component_state.clone(),
                component: component.clone(),
                component_tvl: None,
                entrypoints: Vec::new(),
            };
            let result = quote_component(
                protocol_kind,
                snapshot,
                &tokens,
                sell_token,
                buy_token,
                &amount_in,
            )
            .await;

            if let Ok(result) = result {
                protocol_successes += 1;
                candidates.push(QuoteCandidate {
                    protocol: protocol.clone(),
                    pool: component.id,
                    created_at: component.created_at.to_string(),
                    formatted_amount_out: format_amount(&result.amount, buy_token),
                    amount_out: result.amount.to_string(),
                    gas: result.gas.to_string(),
                });
            }
        }

        if protocol_successes == 0 {
            skipped_protocols.push(SkippedProtocol {
                protocol: protocol.clone(),
                reason: "candidate pools failed to decode or simulate".to_string(),
            });
        }
    }

    candidates.sort_by(|left, right| {
        parse_biguint(&right.amount_out, "candidate.amountOut")
            .unwrap_or_default()
            .cmp(&parse_biguint(&left.amount_out, "candidate.amountOut").unwrap_or_default())
    });

    let successful_quote_count = candidates.len();
    let skipped_count = candidate_count.saturating_sub(successful_quote_count);
    let best_quote = candidates.first().cloned();
    let meets_min_amount = best_quote
        .as_ref()
        .and_then(|quote| parse_biguint(&quote.amount_out, "bestQuote.amountOut").ok())
        .is_some_and(|amount_out| amount_out >= min_amount_out);

    Ok(DirectQuoteResponse {
        sell_token: request.sell_token,
        buy_token: request.buy_token,
        normalized_sell_token: format_address(&normalized_sell_token),
        normalized_buy_token: format_address(&normalized_buy_token),
        wrap_or_unwrap_required,
        amount_in: amount_in.to_string(),
        min_amount_out: min_amount_out.to_string(),
        best_quote,
        meets_min_amount,
        candidate_count,
        successful_quote_count,
        skipped_count,
        candidates: candidates
            .into_iter()
            .take(10)
            .collect(),
        skipped_protocols,
    })
}

async fn quote_component(
    protocol_kind: ProtocolKind,
    snapshot: ComponentWithState,
    tokens: &HashMap<Bytes, Token>,
    sell_token: &Token,
    buy_token: &Token,
    amount_in: &BigUint,
) -> anyhow::Result<tycho_common::simulation::protocol_sim::GetAmountOutResult> {
    match protocol_kind {
        ProtocolKind::V2 => {
            let reserve0 = snapshot
                .state
                .attributes
                .get("reserve0")
                .map(parse_u256)
                .ok_or_else(|| anyhow::anyhow!("reserve0 missing"))?;
            let reserve1 = snapshot
                .state
                .attributes
                .get("reserve1")
                .map(parse_u256)
                .ok_or_else(|| anyhow::anyhow!("reserve1 missing"))?;
            let sim = UniswapV2State::new(reserve0, reserve1);
            Ok(sim.get_amount_out(amount_in.clone(), sell_token, buy_token)?)
        }
        ProtocolKind::V3 => {
            let sim = UniswapV3State::try_from_with_header(
                snapshot,
                BlockHeader::default(),
                &HashMap::new(),
                tokens,
                &DecoderContext::default(),
            )
            .await?;
            Ok(sim.get_amount_out(amount_in.clone(), sell_token, buy_token)?)
        }
        ProtocolKind::V4 => {
            let sim = UniswapV4State::try_from_with_header(
                snapshot,
                BlockHeader::default(),
                &HashMap::new(),
                tokens,
                &DecoderContext::default(),
            )
            .await?;
            Ok(sim.get_amount_out(amount_in.clone(), sell_token, buy_token)?)
        }
        ProtocolKind::Ekubo => {
            let sim = EkuboState::try_from_with_header(
                snapshot,
                BlockHeader::default(),
                &HashMap::new(),
                tokens,
                &DecoderContext::default(),
            )
            .await?;
            Ok(sim.get_amount_out(amount_in.clone(), sell_token, buy_token)?)
        }
    }
}

fn protocol_kind(protocol: &str) -> Option<ProtocolKind> {
    match protocol {
        "uniswap_v2" | "sushiswap_v2" | "pancakeswap_v2" => Some(ProtocolKind::V2),
        "uniswap_v3" | "pancakeswap_v3" => Some(ProtocolKind::V3),
        "uniswap_v4" => Some(ProtocolKind::V4),
        "ekubo_v2" => Some(ProtocolKind::Ekubo),
        _ => None,
    }
}

fn parse_protocols(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_address(value: &str, field: &str) -> anyhow::Result<Bytes> {
    Bytes::from_str(value).map_err(|err| anyhow::anyhow!("{field} must be an address: {err}"))
}

fn normalize_native_token(token: &Bytes) -> anyhow::Result<Bytes> {
    let zero = Bytes::from_str(ZERO_ADDRESS)?;
    let native = Bytes::from_str(NATIVE_ETH_ADDRESS)?;
    if token == &zero || token == &native {
        Bytes::from_str(WETH_ADDRESS).map_err(Into::into)
    } else {
        Ok(token.clone())
    }
}

fn parse_biguint(value: &str, field: &str) -> anyhow::Result<BigUint> {
    BigUint::from_str(value)
        .map_err(|err| anyhow::anyhow!("{field} must be a decimal integer: {err}"))
}

fn has_token_pair(component: &ProtocolComponent, token_a: &Bytes, token_b: &Bytes) -> bool {
    component
        .tokens
        .iter()
        .any(|token| token == token_a)
        && component
            .tokens
            .iter()
            .any(|token| token == token_b)
}

fn parse_u256(bytes: &Bytes) -> U256 {
    U256::from_be_slice(bytes.as_ref())
}

fn format_address(address: &Bytes) -> String {
    format!("{address:#x}")
}

fn format_amount(amount: &BigUint, token: &Token) -> String {
    let decimal = amount
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 10f64.powi(token.decimals as i32);
    format!("{decimal:.12}")
}

fn internal_error(error: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: error.to_string() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_eth_normalizes_to_weth() {
        let zero = Bytes::from_str(ZERO_ADDRESS).unwrap();
        let native = Bytes::from_str(NATIVE_ETH_ADDRESS).unwrap();
        let weth = Bytes::from_str(WETH_ADDRESS).unwrap();

        assert_eq!(normalize_native_token(&zero).unwrap(), weth);
        assert_eq!(normalize_native_token(&native).unwrap(), weth);
    }

    #[test]
    fn protocol_list_parser_trims_empty_values() {
        assert_eq!(
            parse_protocols(" uniswap_v3,, sushiswap_v2 "),
            vec!["uniswap_v3".to_string(), "sushiswap_v2".to_string()]
        );
    }

    #[test]
    fn parses_raw_amounts_only() {
        assert_eq!(parse_biguint("1000000", "amountIn").unwrap(), BigUint::from(1_000_000u32));
        assert!(parse_biguint("1.0", "amountIn").is_err());
    }
}
