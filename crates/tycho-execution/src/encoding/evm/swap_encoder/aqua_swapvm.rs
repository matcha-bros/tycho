use std::collections::HashMap;

use tycho_common::{models::Chain, Bytes};

use crate::encoding::{
    errors::EncodingError,
    evm::utils::get_static_attribute,
    models::{EncodingContext, Swap},
    swap_encoder::SwapEncoder,
};

const ATTR_ROUTER: &str = "router";
const ATTR_MAKER: &str = "maker";
const ATTR_ORDER_TRAITS: &str = "order_traits";
const ATTR_ORDER_DATA: &str = "order_data";
const PAYLOAD_VERSION: u8 = 1;

#[derive(Clone)]
pub struct AquaSwapVMSwapEncoder {
    executor_address: Bytes,
}

impl SwapEncoder for AquaSwapVMSwapEncoder {
    fn new(
        executor_address: Bytes,
        _chain: Chain,
        _config: Option<HashMap<String, String>>,
    ) -> Result<Self, EncodingError> {
        Ok(Self { executor_address })
    }

    fn encode_swap(
        &self,
        swap: &Swap,
        _encoding_context: &EncodingContext,
    ) -> Result<Vec<u8>, EncodingError> {
        let router = fixed_attribute::<20>(swap, ATTR_ROUTER)?;
        let maker = fixed_attribute::<20>(swap, ATTR_MAKER)?;
        let order_traits = u256_attribute(swap, ATTR_ORDER_TRAITS)?;
        let token_in = fixed_bytes::<20>(swap.token_in().address.as_ref(), "token_in")?;
        let token_out = fixed_bytes::<20>(swap.token_out().address.as_ref(), "token_out")?;
        let order_data = get_static_attribute(swap, ATTR_ORDER_DATA)?;

        if order_data.is_empty() {
            return Err(EncodingError::FatalError(
                "Aqua SwapVM order_data must not be empty".to_string(),
            ));
        }

        let mut encoded = Vec::with_capacity(113 + order_data.len());
        encoded.push(PAYLOAD_VERSION);
        encoded.extend_from_slice(&router);
        encoded.extend_from_slice(&maker);
        encoded.extend_from_slice(&order_traits);
        encoded.extend_from_slice(&token_in);
        encoded.extend_from_slice(&token_out);
        encoded.extend_from_slice(&order_data);
        Ok(encoded)
    }

    fn executor_address(&self) -> &Bytes {
        &self.executor_address
    }

    fn clone_box(&self) -> Box<dyn SwapEncoder> {
        Box::new(self.clone())
    }
}

fn fixed_attribute<const N: usize>(
    swap: &Swap,
    attribute_name: &str,
) -> Result<[u8; N], EncodingError> {
    fixed_bytes(&get_static_attribute(swap, attribute_name)?, attribute_name)
}

fn fixed_bytes<const N: usize>(bytes: &[u8], name: &str) -> Result<[u8; N], EncodingError> {
    bytes
        .try_into()
        .map_err(|_| EncodingError::FatalError(format!("Aqua SwapVM {name} must be {N} bytes")))
}

fn u256_attribute(swap: &Swap, attribute_name: &str) -> Result<[u8; 32], EncodingError> {
    let bytes = get_static_attribute(swap, attribute_name)?;
    if bytes.len() > 32 {
        return Err(EncodingError::FatalError(format!(
            "Aqua SwapVM {attribute_name} must be at most 32 bytes"
        )));
    }

    let mut padded = [0u8; 32];
    padded[32 - bytes.len()..].copy_from_slice(&bytes);
    Ok(padded)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use alloy::hex::encode;
    use num_bigint::BigUint;
    use tycho_common::models::protocol::ProtocolComponent;

    use super::*;
    use crate::encoding::models::{default_token, EncodingContext, Swap};

    #[test]
    fn encodes_versioned_aqua_swapvm_payload() {
        let router = Bytes::from_str("0x1111111111111111111111111111111111111111").unwrap();
        let maker = Bytes::from_str("0x2222222222222222222222222222222222222222").unwrap();
        let token_in = Bytes::from_str("0x3333333333333333333333333333333333333333").unwrap();
        let token_out = Bytes::from_str("0x4444444444444444444444444444444444444444").unwrap();
        let component = ProtocolComponent {
            id: "0xcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd".to_string(),
            protocol_system: "aqua_swapvm".to_string(),
            static_attributes: HashMap::from([
                (ATTR_ROUTER.to_string(), router.clone()),
                (ATTR_MAKER.to_string(), maker.clone()),
                (ATTR_ORDER_TRAITS.to_string(), Bytes::from(vec![0x40])),
                (ATTR_ORDER_DATA.to_string(), Bytes::from(vec![17, 0])),
            ]),
            ..Default::default()
        };
        let swap = Swap::new(
            component,
            default_token(token_in.clone()),
            default_token(token_out.clone()),
            BigUint::ZERO,
        );
        let encoder = AquaSwapVMSwapEncoder::new(
            Bytes::from_str("0x5555555555555555555555555555555555555555").unwrap(),
            Chain::Ethereum,
            None,
        )
        .unwrap();
        let encoded = encoder
            .encode_swap(
                &swap,
                &EncodingContext {
                    router_address: Some(Bytes::zero(20)),
                    group_token_in: token_in,
                    group_token_out: token_out,
                },
            )
            .unwrap();

        assert_eq!(encoded.len(), 115);
        assert_eq!(
            encode(encoded),
            concat!(
                "01",
                "1111111111111111111111111111111111111111",
                "2222222222222222222222222222222222222222",
                "0000000000000000000000000000000000000000000000000000000000000040",
                "3333333333333333333333333333333333333333",
                "4444444444444444444444444444444444444444",
                "1100",
            )
        );
    }
}
