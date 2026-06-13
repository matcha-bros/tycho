use std::collections::HashMap;

use alloy::{
    primitives::{keccak256, Address, B256, U256},
    sol_types::SolValue,
};
use tycho_common::{simulation::errors::SimulationError, Bytes};

use crate::evm::{protocol::vm::utils::get_storage_slot_index_at_key, ContractCompiler};

const POOLS_MAPPING_SLOT: u64 = 0;
const AQUA_BALANCES_MAPPING_SLOT: u64 = 0;

pub(crate) const ATTR_ROUTER: &str = "router";
pub(crate) const ATTR_MAKER: &str = "maker";
pub(crate) const ATTR_ORDER_TRAITS: &str = "order_traits";
pub(crate) const ATTR_ORDER_DATA: &str = "order_data";
pub(crate) fn adapter_storage_from_attrs(
    pool_id: &str,
    attrs: &HashMap<String, Bytes>,
) -> Result<HashMap<U256, U256>, SimulationError> {
    let pool_id = pool_id_bytes(pool_id)?;
    let router = attr_address(attrs, ATTR_ROUTER)?;
    let maker = attr_address(attrs, ATTR_MAKER)?;
    let traits = attr_u256(attrs, ATTR_ORDER_TRAITS)?;
    let data = attrs
        .get(ATTR_ORDER_DATA)
        .ok_or_else(|| missing_attr(ATTR_ORDER_DATA))?
        .as_ref();

    Ok(adapter_storage(pool_id, router, maker, traits, data))
}

pub(crate) fn adapter_storage(
    pool_id: B256,
    router: Address,
    maker: Address,
    traits: U256,
    data: &[u8],
) -> HashMap<U256, U256> {
    let base = mapping_slot_bytes32(pool_id, U256::from(POOLS_MAPPING_SLOT));
    let mut storage = HashMap::new();
    storage.insert(base, address_word(router));
    storage.insert(base + U256::from(1), address_word(maker));
    storage.insert(base + U256::from(2), traits);
    write_bytes(&mut storage, base + U256::from(3), data);
    storage
}

pub(crate) fn aqua_balance_storage_slot(
    maker: Address,
    app: Address,
    strategy_hash: B256,
    token: Address,
) -> U256 {
    let maker_slot = get_storage_slot_index_at_key(
        maker,
        U256::from(AQUA_BALANCES_MAPPING_SLOT),
        ContractCompiler::Solidity,
    );
    let app_slot = get_storage_slot_index_at_key(app, maker_slot, ContractCompiler::Solidity);
    let strategy_slot = mapping_slot_bytes32(strategy_hash, app_slot);
    get_storage_slot_index_at_key(token, strategy_slot, ContractCompiler::Solidity)
}

pub(crate) fn aqua_balance_storage_value(amount: U256, token_count: u8) -> U256 {
    (U256::from(token_count) << 248) | amount
}

fn attr_address(attrs: &HashMap<String, Bytes>, name: &str) -> Result<Address, SimulationError> {
    let bytes = attrs
        .get(name)
        .ok_or_else(|| missing_attr(name))?;
    if bytes.len() != 20 {
        return Err(SimulationError::FatalError(format!(
            "Aqua SwapVM attribute {name} must be 20 bytes, got {}",
            bytes.len()
        )));
    }
    Ok(Address::from_slice(bytes.as_ref()))
}

fn attr_u256(attrs: &HashMap<String, Bytes>, name: &str) -> Result<U256, SimulationError> {
    let bytes = attrs
        .get(name)
        .ok_or_else(|| missing_attr(name))?;
    if bytes.len() > 32 {
        return Err(SimulationError::FatalError(format!(
            "Aqua SwapVM attribute {name} must be at most 32 bytes, got {}",
            bytes.len()
        )));
    }
    Ok(U256::from_be_slice(bytes.as_ref()))
}

fn pool_id_bytes(pool_id: &str) -> Result<B256, SimulationError> {
    let no_prefix = pool_id
        .strip_prefix("0x")
        .unwrap_or(pool_id);
    let bytes = hex::decode(no_prefix)
        .map_err(|e| SimulationError::FatalError(format!("Invalid Aqua SwapVM pool id: {e}")))?;
    if bytes.len() != 32 {
        return Err(SimulationError::FatalError(format!(
            "Aqua SwapVM pool id must be exactly 32 bytes, got {}",
            bytes.len()
        )));
    }
    Ok(B256::from_slice(&bytes))
}

fn mapping_slot_bytes32(key: B256, slot: U256) -> U256 {
    U256::from_be_bytes(keccak256((key, slot).abi_encode()).0)
}

fn address_word(address: Address) -> U256 {
    U256::from_be_slice(address.as_slice())
}

fn write_bytes(storage: &mut HashMap<U256, U256>, slot: U256, data: &[u8]) {
    if data.len() < 32 {
        let mut word = [0u8; 32];
        word[..data.len()].copy_from_slice(data);
        word[31] = (data.len() as u8) * 2;
        storage.insert(slot, U256::from_be_bytes(word));
        return;
    }

    storage.insert(slot, U256::from(data.len() * 2 + 1));
    let base = U256::from_be_bytes(keccak256(slot.to_be_bytes::<32>()).0);
    for (idx, chunk) in data.chunks(32).enumerate() {
        let mut word = [0u8; 32];
        word[..chunk.len()].copy_from_slice(chunk);
        storage.insert(base + U256::from(idx), U256::from_be_bytes(word));
    }
}

fn missing_attr(name: &str) -> SimulationError {
    SimulationError::FatalError(format!("Missing Aqua SwapVM static attribute {name}"))
}

#[cfg(test)]
mod tests {
    use std::{str::FromStr, sync::LazyLock};

    use super::*;

    static SHARED_DB_TEST_LOCK: LazyLock<tokio::sync::Mutex<()>> =
        LazyLock::new(|| tokio::sync::Mutex::new(()));

    const REAL_AQUA_ADDRESS: &str = "0x5aAdFB43eF8dAF45DD80F4676345b7676f1D70e3";
    const MAKER_TRAITS_USE_AQUA: U256 = U256::from_limbs([0, 0, 0, 1u64 << 62]);
    const ONE: u128 = 1_000_000_000_000_000_000;

    #[test]
    fn encodes_aqua_balance_storage() {
        let maker = Address::repeat_byte(0x11);
        let router = Address::repeat_byte(0x22);
        let strategy = B256::repeat_byte(0x33);
        let token = Address::repeat_byte(0x44);
        let slot = aqua_balance_storage_slot(maker, router, strategy, token);
        assert_ne!(slot, U256::ZERO);
        assert_eq!(
            aqua_balance_storage_value(U256::from(5), 2),
            (U256::from(2) << 248) | U256::from(5)
        );
    }

    #[test]
    fn encodes_long_order_data_storage() {
        let storage = adapter_storage(
            B256::repeat_byte(0xaa),
            Address::repeat_byte(0x11),
            Address::repeat_byte(0x22),
            U256::from(3),
            &[0x7b; 64],
        );
        assert!(storage
            .values()
            .any(|value| *value == U256::from(129)));
    }

    #[tokio::test]
    async fn simulates_swapvm_quote_through_tycho_vm_adapter() {
        let _guard = SHARED_DB_TEST_LOCK.lock().await;

        use std::str::FromStr;

        use num_bigint::BigUint;
        use revm::{
            primitives::KECCAK_EMPTY,
            state::{AccountInfo, Bytecode},
        };
        use tycho_client::feed::BlockHeader;
        use tycho_common::{
            models::{token::Token, Chain},
            simulation::protocol_sim::ProtocolSim,
        };

        use crate::evm::{
            engine_db::{
                create_engine, engine_db_interface::EngineDatabaseInterface, SHARED_TYCHO_DB,
            },
            protocol::{
                utils::bytes_to_address,
                vm::{
                    constants::{AQUA_SWAPVM, EXTERNAL_ACCOUNT, MAX_BALANCE},
                    state_builder::EVMPoolStateBuilder,
                },
            },
        };

        SHARED_TYCHO_DB
            .clear()
            .expect("failed to clear shared tycho db");
        SHARED_TYCHO_DB
            .update(vec![], Some(BlockHeader { number: 1, timestamp: 1, ..Default::default() }))
            .expect("failed to set test block");

        let adapter = Address::from_str("0x1000000000000000000000000000000000000001").unwrap();
        let router = Address::from_str("0x2000000000000000000000000000000000000002").unwrap();
        let maker = Address::from_str("0x3000000000000000000000000000000000000003").unwrap();
        let token_in = Bytes::from_str("0x4000000000000000000000000000000000000004").unwrap();
        let token_out = Bytes::from_str("0x5000000000000000000000000000000000000005").unwrap();
        let pool_id =
            "0xabababababababababababababababababababababababababababababababab".to_string();

        let router_bytecode = Bytecode::new_raw(
            include_bytes!("assets/MockAquaSwapVMRouter.evm.runtime")
                .to_vec()
                .into(),
        );
        let engine = create_engine(SHARED_TYCHO_DB.clone(), false).unwrap();
        engine
            .state
            .init_account(
                *EXTERNAL_ACCOUNT,
                AccountInfo {
                    balance: *MAX_BALANCE,
                    nonce: 0,
                    code_hash: KECCAK_EMPTY,
                    code: None,
                },
                None,
                false,
            )
            .unwrap();
        engine
            .state
            .init_account(
                router,
                AccountInfo {
                    balance: U256::ZERO,
                    nonce: 0,
                    code_hash: router_bytecode.hash_slow(),
                    code: Some(router_bytecode),
                },
                None,
                false,
            )
            .unwrap();

        let storage = adapter_storage(
            pool_id_bytes(&pool_id).unwrap(),
            router,
            maker,
            U256::from(42),
            &[1, 2, 3, 4, 5, 6],
        );

        let token_in_model =
            Token::new(&token_in, "TIN", 18, 0, &[Some(10_000)], Chain::Ethereum, 100);
        let token_out_model =
            Token::new(&token_out, "TOUT", 18, 0, &[Some(10_000)], Chain::Ethereum, 100);
        let balances = HashMap::from([
            (bytes_to_address(&token_in).unwrap(), U256::from(1_000_000u64)),
            (bytes_to_address(&token_out).unwrap(), U256::from(1_000_000u64)),
        ]);

        let pool =
            EVMPoolStateBuilder::new(pool_id, vec![token_in.clone(), token_out.clone()], adapter)
                .balances(balances)
                .balance_owner(maker)
                .adapter_contract_bytecode(Bytecode::new_raw(AQUA_SWAPVM.into()))
                .adapter_contract_storage(storage)
                .engine(engine)
                .build(SHARED_TYCHO_DB.clone())
                .await
                .expect("failed to build Aqua SwapVM pool state");

        let quote = ProtocolSim::get_amount_out(
            &pool,
            BigUint::from(5_u64),
            &token_in_model,
            &token_out_model,
        )
        .expect("Aqua SwapVM adapter simulation should quote through router");

        assert_eq!(quote.amount, BigUint::from(10_u64));
    }

    #[tokio::test]
    async fn simulates_real_aqua_swapvm_xyc_quote() {
        let token_in = Bytes::from_str("0x4000000000000000000000000000000000000004").unwrap();
        let token_out = Bytes::from_str("0x5000000000000000000000000000000000000005").unwrap();
        let balance_in = U256::from(1_000_000u64);
        let balance_out = U256::from(2_000_000u64);
        let amount_in = U256::from(100_000u64);

        let quote = simulate_real_aqua_swapvm_quote(
            token_in.clone(),
            token_out.clone(),
            vec![17, 0],
            balance_in,
            balance_out,
            amount_in,
        )
        .await;

        let expected = xyc_amount_out(amount_in, balance_in, balance_out);
        assert_eq!(quote.amount, expected.to::<u128>().into());
    }

    #[tokio::test]
    async fn simulates_real_aqua_swapvm_xyc_concentrate_quote() {
        let token_in = Bytes::from_str("0x4000000000000000000000000000000000000004").unwrap();
        let token_out = Bytes::from_str("0x5000000000000000000000000000000000000005").unwrap();
        let balance_in = U256::from(1_000_000u64);
        let balance_out = U256::from(2_000_000u64);
        let amount_in = U256::from(100_000u64);
        let sqrt_price_min = U256::from(ONE);
        let sqrt_price_max = U256::from(2 * ONE);
        let mut program = vec![18, 64];
        program.extend(sqrt_price_min.to_be_bytes::<32>());
        program.extend(sqrt_price_max.to_be_bytes::<32>());

        let quote = simulate_real_aqua_swapvm_quote(
            token_in.clone(),
            token_out.clone(),
            program,
            balance_in,
            balance_out,
            amount_in,
        )
        .await;

        let expected = xyc_concentrate_amount_out(
            bytes_to_address_for_test(&token_in),
            bytes_to_address_for_test(&token_out),
            amount_in,
            balance_in,
            balance_out,
            sqrt_price_min,
            sqrt_price_max,
        );
        assert_eq!(quote.amount, expected.to::<u128>().into());
    }

    async fn simulate_real_aqua_swapvm_quote(
        token_in: Bytes,
        token_out: Bytes,
        order_data: Vec<u8>,
        balance_in: U256,
        balance_out: U256,
        amount_in: U256,
    ) -> tycho_common::simulation::protocol_sim::GetAmountOutResult {
        let _guard = SHARED_DB_TEST_LOCK.lock().await;

        use std::str::FromStr;

        use num_bigint::BigUint;
        use revm::{
            primitives::KECCAK_EMPTY,
            state::{AccountInfo, Bytecode},
        };
        use tycho_client::feed::BlockHeader;
        use tycho_common::{
            models::{token::Token, Chain},
            simulation::protocol_sim::ProtocolSim,
        };

        use crate::evm::{
            engine_db::{
                create_engine, engine_db_interface::EngineDatabaseInterface, SHARED_TYCHO_DB,
            },
            protocol::{
                utils::bytes_to_address,
                vm::{
                    constants::{AQUA_SWAPVM, EXTERNAL_ACCOUNT, MAX_BALANCE},
                    state_builder::EVMPoolStateBuilder,
                },
            },
        };

        SHARED_TYCHO_DB
            .clear()
            .expect("failed to clear shared tycho db");
        SHARED_TYCHO_DB
            .update(vec![], Some(BlockHeader { number: 1, timestamp: 1, ..Default::default() }))
            .expect("failed to set test block");

        let adapter = Address::from_str("0x1000000000000000000000000000000000000001").unwrap();
        let router = Address::from_str("0x2000000000000000000000000000000000000002").unwrap();
        let maker = Address::from_str("0x3000000000000000000000000000000000000003").unwrap();
        let aqua = Address::from_str(REAL_AQUA_ADDRESS).unwrap();
        let pool_id =
            "0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd".to_string();

        let order_hash = aqua_order_hash(maker, MAKER_TRAITS_USE_AQUA, &order_data);
        let token_in_address = bytes_to_address(&token_in).unwrap();
        let token_out_address = bytes_to_address(&token_out).unwrap();

        let aqua_storage = HashMap::from([
            (
                aqua_balance_storage_slot(maker, router, order_hash, token_in_address),
                aqua_balance_storage_value(balance_in, 2),
            ),
            (
                aqua_balance_storage_slot(maker, router, order_hash, token_out_address),
                aqua_balance_storage_value(balance_out, 2),
            ),
        ]);

        let aqua_bytecode = Bytecode::new_raw(
            include_bytes!("assets/Aqua.evm.runtime")
                .to_vec()
                .into(),
        );
        let router_bytecode = Bytecode::new_raw(
            include_bytes!("assets/AquaSwapVMRouter.evm.runtime")
                .to_vec()
                .into(),
        );
        let engine = create_engine(SHARED_TYCHO_DB.clone(), false).unwrap();
        engine
            .state
            .init_account(
                *EXTERNAL_ACCOUNT,
                AccountInfo {
                    balance: *MAX_BALANCE,
                    nonce: 0,
                    code_hash: KECCAK_EMPTY,
                    code: None,
                },
                None,
                false,
            )
            .unwrap();
        engine
            .state
            .init_account(
                aqua,
                AccountInfo {
                    balance: U256::ZERO,
                    nonce: 0,
                    code_hash: aqua_bytecode.hash_slow(),
                    code: Some(aqua_bytecode),
                },
                Some(aqua_storage),
                false,
            )
            .unwrap();
        engine
            .state
            .init_account(
                router,
                AccountInfo {
                    balance: U256::ZERO,
                    nonce: 0,
                    code_hash: router_bytecode.hash_slow(),
                    code: Some(router_bytecode),
                },
                None,
                false,
            )
            .unwrap();

        let storage = adapter_storage(
            pool_id_bytes(&pool_id).unwrap(),
            router,
            maker,
            MAKER_TRAITS_USE_AQUA,
            &order_data,
        );
        let balances =
            HashMap::from([(token_in_address, balance_in), (token_out_address, balance_out)]);

        let pool =
            EVMPoolStateBuilder::new(pool_id, vec![token_in.clone(), token_out.clone()], adapter)
                .balances(balances)
                .balance_owner(maker)
                .adapter_contract_bytecode(Bytecode::new_raw(AQUA_SWAPVM.into()))
                .adapter_contract_storage(storage)
                .engine(engine)
                .build(SHARED_TYCHO_DB.clone())
                .await
                .expect("failed to build real Aqua SwapVM pool state");

        let token_in_model =
            Token::new(&token_in, "TIN", 18, 0, &[Some(10_000)], Chain::Ethereum, 100);
        let token_out_model =
            Token::new(&token_out, "TOUT", 18, 0, &[Some(10_000)], Chain::Ethereum, 100);

        ProtocolSim::get_amount_out(
            &pool,
            BigUint::from(amount_in.to::<u128>()),
            &token_in_model,
            &token_out_model,
        )
        .expect("real Aqua SwapVM adapter simulation should quote through router")
    }

    fn aqua_order_hash(maker: Address, traits: U256, data: &[u8]) -> B256 {
        keccak256((maker, traits, data.to_vec()).abi_encode())
    }

    fn xyc_amount_out(amount_in: U256, balance_in: U256, balance_out: U256) -> U256 {
        (amount_in * balance_out) / (balance_in + amount_in)
    }

    fn xyc_concentrate_amount_out(
        token_in: Address,
        token_out: Address,
        amount_in: U256,
        balance_in: U256,
        balance_out: U256,
        sqrt_price_min: U256,
        sqrt_price_max: U256,
    ) -> U256 {
        use crate::evm::protocol::{
            safe_math::sqrt_u256,
            utils::solidity_math::{mul_div, mul_div_rounding_up},
        };

        let is_token_in_lt = token_in < token_out;
        let balance_lt = if is_token_in_lt { balance_in } else { balance_out };
        let balance_gt = if is_token_in_lt { balance_out } else { balance_in };
        let price_delta = sqrt_price_max - sqrt_price_min;
        let beta = mul_div(balance_lt, sqrt_price_min, U256::from(ONE)).unwrap()
            + mul_div(balance_gt, U256::from(ONE), sqrt_price_max).unwrap();
        let four_ac =
            mul_div(U256::from(4) * price_delta, balance_lt * balance_gt, sqrt_price_max).unwrap();
        let disc = beta * beta + four_ac;
        let liquidity =
            mul_div(beta + sqrt_u256(disc).unwrap(), sqrt_price_max, U256::from(2) * price_delta)
                .unwrap();

        let (virtual_balance_in, virtual_balance_out) = if is_token_in_lt {
            (
                balance_in
                    + mul_div_rounding_up(liquidity, U256::from(ONE), sqrt_price_max).unwrap(),
                balance_out + mul_div(liquidity, sqrt_price_min, U256::from(ONE)).unwrap(),
            )
        } else {
            (
                balance_in
                    + mul_div_rounding_up(liquidity, sqrt_price_min, U256::from(ONE)).unwrap(),
                balance_out + mul_div(liquidity, U256::from(ONE), sqrt_price_max).unwrap(),
            )
        };

        xyc_amount_out(amount_in, virtual_balance_in, virtual_balance_out)
    }

    fn bytes_to_address_for_test(bytes: &Bytes) -> Address {
        use crate::evm::protocol::utils::bytes_to_address;

        bytes_to_address(bytes).unwrap()
    }
}
