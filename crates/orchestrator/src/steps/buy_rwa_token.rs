use crate::steps::{StepContext, StepOutput};
use common::{EngineError, WorkflowStep};
use tracing::info;

pub async fn execute(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;

    if position.collateral_amount.is_some() {
        info!(position_id = %position.id, "buy_rwa_token: already executed, skipping");
        return Ok(StepOutput {
            tx_hash:          position.last_tx_hash.clone().unwrap_or_default(),
            collateral_delta: position.collateral_amount,
            debt_delta:       None,
        });
    }

    let calldata = encode_buy_call(&ctx.params.rwa_token, ctx.params.initial_collateral)?;

    info!(position_id = %position.id, step = %WorkflowStep::BuyRwaToken, "submitting tx");

    let tx_hash = ctx
        .chain
        .send_transaction(&ctx.params.rwa_token, calldata)
        .await
        .map_err(|e| EngineError::StepFailed {
            id:     position.id,
            step:   WorkflowStep::BuyRwaToken.to_string(),
            reason: e.to_string(),
        })?;

    if !ctx.chain.wait_for_receipt(&tx_hash, ctx.tx_timeout_secs).await? {
        return Err(EngineError::TxReverted { hash: tx_hash, reason: "buy_rwa_token reverted".into() });
    }

    info!(position_id = %position.id, tx_hash = %tx_hash, "buy_rwa_token confirmed");

    Ok(StepOutput {
        tx_hash,
        collateral_delta: Some(ctx.params.initial_collateral),
        debt_delta:       None,
    })
}

fn encode_buy_call(_rwa_token: &str, _amount: rust_decimal::Decimal) -> Result<Vec<u8>, EngineError> {
    // TODO: encode swap/mint call (e.g. Uniswap or direct Centrifuge invest).
    Ok(vec![])
}
