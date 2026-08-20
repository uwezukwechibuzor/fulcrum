use crate::steps::{StepContext, StepOutput};
use common::{EngineError, WorkflowStep};
use rust_decimal::Decimal;
use tracing::info;

pub async fn repay_debt(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;
    let debt = position.debt_amount.unwrap_or_default();

    if debt == Decimal::ZERO {
        info!(position_id = %position.id, "repay_debt: no debt, skipping");
        return Ok(StepOutput { tx_hash: String::new(), collateral_delta: position.collateral_amount, debt_delta: Some(Decimal::ZERO) });
    }

    let calldata = encode_repay(&ctx.params.market_id, &ctx.params.owner_address, debt)?;
    let tx_hash  = submit(ctx, &ctx.params.facility, calldata, WorkflowStep::RepayDebt).await?;
    info!(position_id = %position.id, tx_hash = %tx_hash, "repay_debt confirmed");

    Ok(StepOutput { tx_hash, collateral_delta: position.collateral_amount, debt_delta: Some(Decimal::ZERO) })
}

pub async fn withdraw_collateral(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position   = ctx.position;
    let collateral = position.collateral_amount.unwrap_or_default();

    let calldata = encode_withdraw(&ctx.params.market_id, &ctx.params.owner_address, collateral)?;
    let tx_hash  = submit(ctx, &ctx.params.facility, calldata, WorkflowStep::WithdrawCollateral).await?;
    info!(position_id = %position.id, tx_hash = %tx_hash, "withdraw_collateral confirmed");

    Ok(StepOutput { tx_hash, collateral_delta: Some(Decimal::ZERO), debt_delta: Some(Decimal::ZERO) })
}

pub async fn sell_rwa_token(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;
    let calldata = encode_sell(&ctx.params.rwa_token, ctx.params.initial_collateral)?;
    let tx_hash  = submit(ctx, &ctx.params.rwa_token, calldata, WorkflowStep::SellRwaToken).await?;
    info!(position_id = %position.id, tx_hash = %tx_hash, "sell_rwa_token confirmed");

    Ok(StepOutput { tx_hash, collateral_delta: Some(Decimal::ZERO), debt_delta: None })
}

pub async fn repay_bridge_loan(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;
    let calldata = encode_repay_bridge(&ctx.params.owner_address)?;
    let tx_hash  = submit(ctx, &ctx.params.facility, calldata, WorkflowStep::RepayBridgeLoan).await?;
    info!(position_id = %position.id, tx_hash = %tx_hash, "repay_bridge_loan confirmed — position fully closed");

    Ok(StepOutput { tx_hash, collateral_delta: Some(Decimal::ZERO), debt_delta: Some(Decimal::ZERO) })
}

// ── Shared submit + confirm ───────────────────────────────────────────────────

async fn submit(
    ctx: &StepContext<'_>,
    to: &str,
    calldata: Vec<u8>,
    step: WorkflowStep,
) -> Result<String, EngineError> {
    let position = ctx.position;
    info!(position_id = %position.id, step = %step, "submitting tx");

    let tx_hash = ctx
        .chain
        .send_transaction(to, calldata)
        .await
        .map_err(|e| EngineError::StepFailed {
            id:     position.id,
            step:   step.to_string(),
            reason: e.to_string(),
        })?;

    if !ctx.chain.wait_for_receipt(&tx_hash, ctx.tx_timeout_secs).await? {
        return Err(EngineError::TxReverted { hash: tx_hash, reason: format!("{step} reverted") });
    }
    Ok(tx_hash)
}

// ── Calldata encoders ─────────────────────────────────────────────────────────

fn encode_repay(_market_id: &str, _owner: &str, _amount: Decimal)    -> Result<Vec<u8>, EngineError> { Ok(vec![]) }
fn encode_withdraw(_market_id: &str, _owner: &str, _amount: Decimal) -> Result<Vec<u8>, EngineError> { Ok(vec![]) }
fn encode_sell(_token: &str, _amount: Decimal)                        -> Result<Vec<u8>, EngineError> { Ok(vec![]) }
fn encode_repay_bridge(_borrower: &str)                               -> Result<Vec<u8>, EngineError> { Ok(vec![]) }
