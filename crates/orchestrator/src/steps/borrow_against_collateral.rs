use crate::steps::{StepContext, StepOutput};
use common::{EngineError, WorkflowStep};
use rust_decimal::Decimal;
use tracing::info;

pub async fn execute(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;
    let collateral = position.collateral_amount.unwrap_or_default();
    let borrow_amount = collateral * (ctx.params.target_leverage - Decimal::ONE);

    if borrow_amount <= Decimal::ZERO {
        return Err(EngineError::StepFailed {
            id:     position.id,
            step:   WorkflowStep::BorrowAgainstCollateral.to_string(),
            reason: "borrow amount is zero — check leverage config".into(),
        });
    }

    let calldata = encode_borrow(&ctx.params.market_id, &ctx.params.owner_address, borrow_amount)?;

    info!(
        position_id = %position.id,
        step = %WorkflowStep::BorrowAgainstCollateral,
        borrow_amount = %borrow_amount,
        "submitting tx"
    );

    let tx_hash = ctx
        .chain
        .send_transaction(&ctx.params.facility, calldata)
        .await
        .map_err(|e| EngineError::StepFailed {
            id:     position.id,
            step:   WorkflowStep::BorrowAgainstCollateral.to_string(),
            reason: e.to_string(),
        })?;

    if !ctx.chain.wait_for_receipt(&tx_hash, ctx.tx_timeout_secs).await? {
        return Err(EngineError::TxReverted { hash: tx_hash, reason: "borrow_against_collateral reverted".into() });
    }

    let total_debt = position.debt_amount.unwrap_or_default() + borrow_amount;
    info!(position_id = %position.id, tx_hash = %tx_hash, total_debt = %total_debt, "borrow confirmed");

    Ok(StepOutput {
        tx_hash,
        collateral_delta: position.collateral_amount,
        debt_delta:       Some(total_debt),
    })
}

fn encode_borrow(_market_id: &str, _on_behalf_of: &str, _amount: Decimal) -> Result<Vec<u8>, EngineError> {
    // TODO: Morpho.borrow(marketParams, assets, shares, onBehalfOf, receiver)
    Ok(vec![])
}
