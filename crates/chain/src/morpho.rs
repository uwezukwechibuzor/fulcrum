/// Morpho Blue on-chain reads.
///
/// Health factor formula (from the Morpho whitepaper):
///   collateral_value = collateral * oracle_price / 1e36
///   debt_value       = borrow_shares * total_borrow_assets / total_borrow_shares
///   health_factor    = (collateral_value * lltv / 1e18) / debt_value
use alloy::{
    primitives::{Address, Bytes, FixedBytes},
    sol,
    sol_types::SolCall,
};
use common::{EngineError, MorphoMarket, MorphoPosition};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

use crate::client::EthClient;

sol! {
    interface IMorpho {
        struct MarketParams {
            address loanToken;
            address collateralToken;
            address oracle;
            address irm;
            uint256 lltv;
        }

        struct Market {
            uint128 totalSupplyAssets;
            uint128 totalSupplyShares;
            uint128 totalBorrowAssets;
            uint128 totalBorrowShares;
            uint128 lastUpdate;
            uint128 fee;
        }

        struct Position {
            uint256 supplyShares;
            uint128 borrowShares;
            uint128 collateral;
        }

        function market(bytes32 id) external view returns (Market memory);
        function position(bytes32 id, address user) external view returns (Position memory);
        function idToMarketParams(bytes32 id) external view returns (MarketParams memory);
    }

    interface IOracle {
        function price() external view returns (uint256);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_market_id(market_id: &str) -> Result<FixedBytes<32>, EngineError> {
    let s = market_id.strip_prefix("0x").unwrap_or(market_id);
    let bytes = alloy::primitives::hex::decode(s)
        .map_err(|e| EngineError::Rpc(format!("invalid market_id hex: {e}")))?;
    if bytes.len() != 32 {
        return Err(EngineError::Rpc("market_id must be 32 bytes".into()));
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Ok(FixedBytes::<32>::from(arr))
}

async fn eth_call(
    client: &EthClient,
    to: Address,
    calldata: Vec<u8>,
) -> Result<Bytes, EngineError> {
    client.eth_call(to, &calldata).await
}

// ── Public reads ──────────────────────────────────────────────────────────────

pub async fn get_position(
    client: &EthClient,
    market_id: &str,
    user_address: &str,
) -> Result<MorphoPosition, EngineError> {
    let id   = parse_market_id(market_id)?;
    let user = user_address
        .parse::<Address>()
        .map_err(|e| EngineError::Rpc(format!("invalid user address: {e}")))?;

    let calldata = IMorpho::positionCall { id, user }.abi_encode();
    let raw = eth_call(client, client.morpho_address, calldata).await?;

    // In alloy 2.x single-return sol! functions decode directly to the return type.
    let pos = IMorpho::positionCall::abi_decode_returns(&raw)
        .map_err(|e| EngineError::Rpc(format!("decode position: {e}")))?;

    Ok(MorphoPosition {
        supply_shares: pos.supplyShares.try_into().unwrap_or(u128::MAX),
        borrow_shares: pos.borrowShares,
        collateral:    pos.collateral,
    })
}

pub async fn get_market(
    client: &EthClient,
    market_id: &str,
) -> Result<MorphoMarket, EngineError> {
    let id = parse_market_id(market_id)?;

    // Market state
    let market_calldata = IMorpho::marketCall { id }.abi_encode();
    let market_raw = eth_call(client, client.morpho_address, market_calldata).await?;
    let market = IMorpho::marketCall::abi_decode_returns(&market_raw)
        .map_err(|e| EngineError::Rpc(format!("decode market: {e}")))?;

    // Market params (for oracle address and lltv)
    let params_calldata = IMorpho::idToMarketParamsCall { id }.abi_encode();
    let params_raw = eth_call(client, client.morpho_address, params_calldata).await?;
    let params = IMorpho::idToMarketParamsCall::abi_decode_returns(&params_raw)
        .map_err(|e| EngineError::Rpc(format!("decode market params: {e}")))?;

    // Oracle price (Morpho uses 1e36-scaled price)
    let oracle_calldata = IOracle::priceCall {}.abi_encode();
    let oracle_raw = eth_call(client, params.oracle, oracle_calldata).await?;
    let oracle_price = IOracle::priceCall::abi_decode_returns(&oracle_raw)
        .map_err(|e| EngineError::Rpc(format!("decode oracle price: {e}")))?;

    Ok(MorphoMarket {
        total_supply_assets: market.totalSupplyAssets,
        total_supply_shares: market.totalSupplyShares,
        total_borrow_assets: market.totalBorrowAssets,
        total_borrow_shares: market.totalBorrowShares,
        lltv:                params.lltv.try_into().unwrap_or(u128::MAX),
        oracle_price:        oracle_price.try_into().unwrap_or(u128::MAX),
    })
}

pub async fn compute_health_factor(
    client: &EthClient,
    market_id: &str,
    user_address: &str,
) -> Result<Option<Decimal>, EngineError> {
    let pos    = get_position(client, market_id, user_address).await?;
    let market = get_market(client, market_id).await?;

    if pos.borrow_shares == 0 {
        return Ok(None);
    }

    let debt_value = if market.total_borrow_shares == 0 {
        Decimal::ZERO
    } else {
        Decimal::from(pos.borrow_shares)
            * Decimal::from(market.total_borrow_assets)
            / Decimal::from(market.total_borrow_shares)
    };

    if debt_value == Decimal::ZERO {
        return Ok(None);
    }

    // collateral_value = collateral * oracle_price / 1e36
    let one_e36         = Decimal::from(10u128).powi(36);
    let collateral_value = Decimal::from(pos.collateral)
        * Decimal::from(market.oracle_price)
        / one_e36;

    // health_factor = collateral_value * lltv / 1e18 / debt_value
    let one_e18 = Decimal::from(10u64).powi(18);
    let hf      = (collateral_value * Decimal::from(market.lltv) / one_e18) / debt_value;

    Ok(Some(hf.round_dp(4)))
}
