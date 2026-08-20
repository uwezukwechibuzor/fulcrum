use crate::steps::{StepContext, StepOutput};
use common::{EngineError, WorkflowStep};
use rust_decimal::Decimal;
use tracing::info;

pub async fn execute(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    deposit(ctx, ctx.params.initial_collateral, WorkflowStep::DepositCollateral).await
}

pub async fn top_up(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let current = ctx.position.collateral_amount.unwrap_or_default();
    let amount = current * Decimal::new(10, 2); // add 10%; real impl reads Morpho market
    deposit(ctx, amount, WorkflowStep::TopUpCollateral).await
}

async fn deposit(ctx: &StepContext<'_>, amount: Decimal, step: WorkflowStep) -> Result<StepOutput, EngineError> {
    let position = ctx.position;
    let calldata = encode_supply_collateral(&ctx.params.market_id, &ctx.params.owner_address, amount)?;

    info!(position_id = %position.id, step = %step, amount = %amount, "submitting deposit_collateral tx");

    let tx_hash = ctx
        .chain
        .send_transaction(&ctx.params.facility, calldata)
        .await
        .map_err(|e| EngineError::StepFailed {
            id:     position.id,
            step:   step.to_string(),
            reason: e.to_string(),
        })?;

    if !ctx.chain.wait_for_receipt(&tx_hash, ctx.tx_timeout_secs).await? {
        return Err(EngineError::TxReverted { hash: tx_hash, reason: format!("{step} reverted") });
    }

    let new_collateral = position.collateral_amount.unwrap_or_default() + amount;
    info!(position_id = %position.id, tx_hash = %tx_hash, collateral = %new_collateral, "deposit confirmed");

    Ok(StepOutput { tx_hash, collateral_delta: Some(new_collateral), debt_delta: None })
}

fn encode_supply_collateral(_market_id: &str, _on_behalf_of: &str, _amount: Decimal) -> Result<Vec<u8>, EngineError> {
    // TODO: Morpho.supplyCollateral(marketParams, assets, onBehalfOf, data)
    Ok(vec![])
}
