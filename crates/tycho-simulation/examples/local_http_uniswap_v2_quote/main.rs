use std::{collections::HashMap, env, str::FromStr};

use alloy::primitives::{Address, U256};
use num_bigint::BigUint;
use tycho_simulation::{
    evm::protocol::{
        vm::state::EVMPoolState,
        ekubo::state::EkuboState, uniswap_v2::state::UniswapV2State,
        uniswap_v3::state::UniswapV3State, uniswap_v4::state::UniswapV4State,
    },
    evm::{
        engine_db::{tycho_db::PreCachedDB, update_engine, SHARED_TYCHO_DB},
        protocol::vm::constants::ERC20_PROXY_BYTECODE,
        tycho_models::{AccountUpdate, ChangeType, ResponseAccount},
    },
    protocol::models::{DecoderContext, TryFromWithBlock},
    tycho_client::{
        feed::{synchronizer::ComponentWithState, BlockHeader},
        HttpRPCClient,
        rpc::{
            ContractStatePaginatedParams, HttpRPCClientOptions,
            ProtocolComponentsPaginatedParams, ProtocolComponentsParams, ProtocolStatesParams,
            RPCClient, TokensParams,
        },
    },
    tycho_common::{
        Bytes,
        models::protocol::ProtocolComponent,
        models::{Chain, token::Token},
        simulation::{
            errors::SimulationError,
            protocol_sim::ProtocolSim,
        },
    },
};

const DEFAULT_POOL: &str = "0xb4e16d0168e52d35cacd2c6185b44281ec28c9dc";
const DEFAULT_SELL_TOKEN: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
const DEFAULT_BUY_TOKEN: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";

#[tokio::main]
async fn main() {
    let tycho_url = env::var("TYCHO_URL").unwrap_or_else(|_| "localhost:4242".to_string());
    let auth_key = env::var("TYCHO_API_KEY")
        .or_else(|_| env::var("AUTH_API_KEY"))
        .ok();
    let pool_id = env::var("POOL_ID").unwrap_or_else(|_| DEFAULT_POOL.to_string());
    let sell_token_address =
        Bytes::from_str(&env::var("SELL_TOKEN").unwrap_or_else(|_| DEFAULT_SELL_TOKEN.to_string()))
            .expect("SELL_TOKEN must be an address");
    let buy_token_address =
        Bytes::from_str(&env::var("BUY_TOKEN").unwrap_or_else(|_| DEFAULT_BUY_TOKEN.to_string()))
            .expect("BUY_TOKEN must be an address");
    let sell_amount = env::var("SELL_AMOUNT")
        .unwrap_or_else(|_| "1".to_string())
        .parse::<f64>()
        .unwrap();
    let scan_protocols = env::var("SCAN_PROTOCOLS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let scan_debug = env::var("SCAN_DEBUG").is_ok_and(|value| value == "1" || value == "true");
    let token_min_quality = env::var("TOKEN_MIN_QUALITY")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<i32>()
        .expect("TOKEN_MIN_QUALITY must be an integer");
    let max_days_since_last_trade = env::var("MAX_DAYS_SINCE_LAST_TRADE")
        .unwrap_or_else(|_| "9999".to_string())
        .parse::<u64>()
        .expect("MAX_DAYS_SINCE_LAST_TRADE must be an integer");
    let tvl_gt = env::var("TVL_GT")
        .ok()
        .map(|value| value.parse::<f64>().expect("TVL_GT must be a number"));

    let client = HttpRPCClient::new(
        &format!("http://{tycho_url}"),
        HttpRPCClientOptions::new()
            .with_auth_key(auth_key)
            .with_compression(true),
    )
    .expect("failed to create Tycho RPC client");

    let tokens = client
        .get_tokens(
            TokensParams::new(Chain::Ethereum)
                .with_min_quality(token_min_quality)
                .with_traded_n_days_ago(max_days_since_last_trade),
        )
        .await
        .expect("failed to load tokens")
        .into_data()
        .into_iter()
        .map(|token| (token.address.clone(), token))
        .collect::<HashMap<_, _>>();

    let sell_token = tokens
        .get(&sell_token_address)
        .unwrap_or_else(|| panic!("sell token not found: {sell_token_address:#x}"));
    let buy_token = tokens
        .get(&buy_token_address)
        .unwrap_or_else(|| panic!("buy token not found: {buy_token_address:#x}"));
    let amount_in = BigUint::from((sell_amount * 10f64.powi(sell_token.decimals as i32)) as u128);

    if !scan_protocols.is_empty() {
        scan_protocol_quotes(
            &client,
            &scan_protocols,
            &tokens,
            sell_token,
            buy_token,
            &sell_token_address,
            &buy_token_address,
            &amount_in,
            scan_debug,
            tvl_gt,
        )
        .await;
        return;
    }

    let component = client
        .get_protocol_components(
            ProtocolComponentsParams::new(Chain::Ethereum, "uniswap_v2")
                .with_component_ids(vec![pool_id.clone()]),
        )
        .await
        .expect("failed to load protocol component")
        .into_data()
        .into_iter()
        .next()
        .expect("pool component not found");

    let state = client
        .get_protocol_states(
            ProtocolStatesParams::new(Chain::Ethereum, "uniswap_v2")
                .with_protocol_ids(vec![pool_id.clone()])
                .with_include_balances(true),
        )
        .await
        .expect("failed to load protocol state")
        .into_data()
        .into_iter()
        .next()
        .expect("pool state not found");

    let reserve0 = parse_u256(
        state
            .attributes
            .get("reserve0")
            .expect("reserve0 missing"),
    );
    let reserve1 = parse_u256(
        state
            .attributes
            .get("reserve1")
            .expect("reserve1 missing"),
    );
    let sim = UniswapV2State::new(reserve0, reserve1);
    let result = sim
        .get_amount_out(amount_in.clone(), sell_token, buy_token)
        .expect("quote failed");

    println!("Protocol: {}", component.protocol_system);
    println!("Pool: {}", component.id);
    println!(
        "Swap: {} {} -> {} {}",
        format_amount(&amount_in, sell_token),
        sell_token.symbol,
        format_amount(&result.amount, buy_token),
        buy_token.symbol
    );
    println!("Raw amount out: {}", result.amount);
    println!("Gas: {}", result.gas);
}

async fn scan_protocol_quotes(
    client: &HttpRPCClient,
    protocols: &[String],
    tokens: &HashMap<Bytes, Token>,
    sell_token: &Token,
    buy_token: &Token,
    sell_token_address: &Bytes,
    buy_token_address: &Bytes,
    amount_in: &BigUint,
    scan_debug: bool,
    tvl_gt: Option<f64>,
) {
    let mut quotes = Vec::new();

    for protocol in protocols {
        let protocol_kind = match protocol.as_str() {
            "uniswap_v2" | "sushiswap_v2" | "pancakeswap_v2" => ProtocolKind::V2,
            "uniswap_v3" | "pancakeswap_v3" => ProtocolKind::V3,
            "uniswap_v4" => ProtocolKind::V4,
            "ekubo_v2" => ProtocolKind::Ekubo,
            "vm:balancer_v2" | "vm:curve" => ProtocolKind::Vm,
            _ => {
                println!("{protocol}: scanner does not yet support this protocol family");
                continue;
            }
        };

        let mut component_params = ProtocolComponentsPaginatedParams::new(
            Chain::Ethereum,
            protocol,
            4,
        )
        .with_chunk_size(500);
        if let Some(tvl_gt) = tvl_gt {
            component_params = component_params.with_tvl_gt(tvl_gt);
        }

        let components = client
            .get_protocol_components_paginated(component_params)
            .await
            .unwrap_or_else(|err| panic!("failed to load components for {protocol}: {err}"));

        let candidates = components
            .into_iter()
            .filter(|component| has_token_pair(component, sell_token_address, buy_token_address))
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            println!("{protocol}: no indexed pool for requested token pair");
            continue;
        }

        let states = client
            .get_protocol_states(
                ProtocolStatesParams::new(Chain::Ethereum, protocol)
                    .with_protocol_ids(
                        candidates
                            .iter()
                            .map(|component| component.id.clone())
                            .collect(),
                    )
                    .with_include_balances(true),
            )
            .await
            .unwrap_or_else(|err| panic!("failed to load states for {protocol}: {err}"))
            .into_data()
            .into_iter()
            .map(|state| (state.component_id.clone(), state))
            .collect::<HashMap<_, _>>();

        for component in candidates {
            let Some(state) = states.get(&component.id) else {
                continue;
            };
            let result = match protocol_kind {
                ProtocolKind::V2 => {
                    let (Some(reserve0), Some(reserve1)) = (
                        state
                            .attributes
                            .get("reserve0")
                            .map(parse_u256),
                        state
                            .attributes
                            .get("reserve1")
                            .map(parse_u256),
                    ) else {
                        continue;
                    };
                    let sim = UniswapV2State::new(reserve0, reserve1);
                    sim.get_amount_out(amount_in.clone(), sell_token, buy_token)
                }
                ProtocolKind::V3 => {
                    let snapshot = ComponentWithState {
                        state: state.clone(),
                        component: component.clone(),
                        component_tvl: None,
                        entrypoints: Vec::new(),
                    };
                    match UniswapV3State::try_from_with_header(
                        snapshot,
                        BlockHeader::default(),
                        &HashMap::new(),
                        tokens,
                        &DecoderContext::default(),
                    )
                    .await
                    {
                        Ok(sim) => sim.get_amount_out(amount_in.clone(), sell_token, buy_token),
                        Err(_) => continue,
                    }
                }
                ProtocolKind::V4 => {
                    let snapshot = ComponentWithState {
                        state: state.clone(),
                        component: component.clone(),
                        component_tvl: None,
                        entrypoints: Vec::new(),
                    };
                    match UniswapV4State::try_from_with_header(
                        snapshot,
                        BlockHeader::default(),
                        &HashMap::new(),
                        tokens,
                        &DecoderContext::default(),
                    )
                    .await
                    {
                        Ok(sim) => sim.get_amount_out(amount_in.clone(), sell_token, buy_token),
                        Err(err) => {
                            if scan_debug {
                                eprintln!("{protocol} {} decode failed: {err}", component.id);
                            }
                            continue;
                        }
                    }
                }
                ProtocolKind::Ekubo => {
                    let snapshot = ComponentWithState {
                        state: state.clone(),
                        component: component.clone(),
                        component_tvl: None,
                        entrypoints: Vec::new(),
                    };
                    match EkuboState::try_from_with_header(
                        snapshot,
                        BlockHeader::default(),
                        &HashMap::new(),
                        tokens,
                        &DecoderContext::default(),
                    )
                    .await
                    {
                        Ok(sim) => sim.get_amount_out(amount_in.clone(), sell_token, buy_token),
                        Err(err) => {
                            if scan_debug {
                                eprintln!("{protocol} {} decode failed: {err}", component.id);
                            }
                            continue;
                        }
                    }
                }
                ProtocolKind::Vm => {
                    let snapshot = ComponentWithState {
                        state: state.clone(),
                        component: component.clone(),
                        component_tvl: None,
                        entrypoints: Vec::new(),
                    };
                    match decode_vm_state(client, protocol, snapshot, tokens).await {
                        Ok(sim) => sim.get_amount_out(amount_in.clone(), sell_token, buy_token),
                        Err(err) => {
                            if scan_debug {
                                eprintln!("{protocol} {} decode failed: {err}", component.id);
                            }
                            continue;
                        }
                    }
                }
            };

            match result {
                Ok(result) => {
                    quotes.push((
                        protocol.clone(),
                        component.id,
                        component.created_at,
                        result.amount,
                        result.gas,
                    ));
                }
                Err(err) if scan_debug => {
                    eprintln!("{protocol} {} quote failed: {err}", component.id);
                }
                Err(_) => {}
            }
        }
    }

    quotes.sort_by(|left, right| right.3.cmp(&left.3));

    if quotes.is_empty() {
        println!("No successful quote for requested token pair");
        return;
    }

    println!(
        "Best quotes for {} {} -> {}:",
        format_amount(amount_in, sell_token),
        sell_token.symbol,
        buy_token.symbol
    );
    for (protocol, pool, created_at, amount_out, gas) in quotes.iter().take(10) {
        println!(
            "{protocol:16} {pool} created_at={created_at} out={} {} raw={} gas={gas}",
            format_amount(amount_out, buy_token),
            buy_token.symbol,
            amount_out,
        );
    }
}

#[derive(Clone, Copy)]
enum ProtocolKind {
    V2,
    V3,
    V4,
    Ekubo,
    Vm,
}

async fn decode_vm_state(
    client: &HttpRPCClient,
    protocol: &str,
    snapshot: ComponentWithState,
    tokens: &HashMap<Bytes, Token>,
) -> Result<EVMPoolState<PreCachedDB>, SimulationError> {
    let contract_ids = snapshot
        .component
        .contract_addresses
        .clone();

    let accounts = if contract_ids.is_empty() {
        Vec::new()
    } else {
        client
            .get_contract_state_paginated(
                ContractStatePaginatedParams::new(Chain::Ethereum, protocol, 4)
                    .with_contract_ids(contract_ids)
                    .with_chunk_size(100),
            )
            .await
            .map_err(|err| SimulationError::FatalError(err.to_string()))?
    };

    let account_balances = accounts
        .iter()
        .filter(|account| !account.token_balances.is_empty())
        .map(|account| {
            (
                account.address.clone(),
                account
                    .token_balances
                    .iter()
                    .map(|(token, balance)| (token.clone(), balance.balance.clone()))
                    .collect::<HashMap<_, _>>(),
            )
        })
        .collect::<HashMap<_, _>>();

    let vm_storage = accounts
        .into_iter()
        .map(|account| {
            let response = ResponseAccount::from(account);
            (response.address, response)
        })
        .collect::<HashMap<Address, ResponseAccount>>();

    let mut token_proxy_updates = HashMap::new();
    for token in &snapshot.component.tokens {
        let address = Address::from_slice(token.as_ref());
        token_proxy_updates.entry(address).or_insert_with(|| {
            AccountUpdate::new(
                address,
                Chain::Ethereum,
                HashMap::new(),
                None,
                Some(ERC20_PROXY_BYTECODE.to_vec()),
                ChangeType::Creation,
            )
        });
    }

    update_engine(
        SHARED_TYCHO_DB.clone(),
        Some(BlockHeader::default()),
        Some(vm_storage),
        token_proxy_updates,
    )
    .map_err(|err| SimulationError::FatalError(err.to_string()))?;

    EVMPoolState::<PreCachedDB>::try_from_with_header(
        snapshot,
        BlockHeader::default(),
        &account_balances,
        tokens,
        &DecoderContext::default(),
    )
    .await
    .map_err(|err| SimulationError::FatalError(err.to_string()))
}

fn has_token_pair(component: &ProtocolComponent, token_a: &Bytes, token_b: &Bytes) -> bool {
    component.tokens.iter().any(|token| token == token_a)
        && component.tokens.iter().any(|token| token == token_b)
}

fn parse_u256(bytes: &Bytes) -> U256 {
    U256::from_be_slice(bytes.as_ref())
}

fn format_amount(amount: &BigUint, token: &Token) -> String {
    let decimal = amount
        .to_string()
        .parse::<f64>()
        .unwrap_or(0.0)
        / 10f64.powi(token.decimals as i32);
    format!("{decimal:.12}")
}
