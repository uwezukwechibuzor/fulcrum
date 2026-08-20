use crate::steps::{StepContext, StepOutput};
use common::{EngineError, WorkflowStep};
use tracing::info;

pub async fn execute(ctx: &StepContext<'_>) -> Result<StepOutput, EngineError> {
    let position = ctx.position;

    // Idempotency: if the last tx hash already confirmed, skip re-submission.
    if let Some(ref hash) = position.last_tx_hash {
        if ctx.chain.wait_for_receipt(hash, 5).await.unwrap_or(false) {
            info!(position_id = %position.id, tx_hash = %hash, "borrow_stablecoins: already confirmed, skipping");
            return Ok(StepOutput {
                tx_hash:          hash.clone(),
                collateral_delta: None,
                debt_delta:       Some(ctx.params.initial_collateral),
            });
        }
    }

    let calldata = encode_borrow_call(&ctx.params.owner_address, ctx.params.initial_collateral)?;

    info!(position_id = %position.id, step = %WorkflowStep::BorrowStablecoins, "submitting tx");

    let tx_hash = ctx
        .chain
        .send_transaction(&ctx.params.facility, calldata)
        .await
        .map_err(|e| EngineError::StepFailed {
            id:     position.id,
            step:   WorkflowStep::BorrowStablecoins.to_string(),
            reason: e.to_string(),
        })?;

    if !ctx.chain.wait_for_receipt(&tx_hash, ctx.tx_timeout_secs).await? {
        return Err(EngineError::TxReverted { hash: tx_hash, reason: "borrow_stablecoins reverted".into() });
    }

    info!(position_id = %position.id, tx_hash = %tx_hash, "borrow_stablecoins confirmed");

    Ok(StepOutput {
        tx_hash,
        collateral_delta: None,
        debt_delta:       Some(ctx.params.initial_collateral),
    })
}

fn encode_borrow_call(
    _borrower: &str,
    _amount: rust_decimal::Decimal,
) -> Result<Vec<u8>, EngineError> {
    // TODO: encode ABI call to your bridge financing contract.
    Ok(vec![])
}
