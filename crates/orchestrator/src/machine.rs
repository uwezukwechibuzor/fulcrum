/// State machine: the only place that decides whether a transition is legal
/// and persists both the new state and an audit event atomically.
use common::{EngineError, Position, PositionState, WorkflowStep};
use db::queries;
use sqlx::PgPool;
use uuid::Uuid;

/// Transition a position to a new state, recording the event atomically.
/// Returns `IllegalTransition` without touching the DB if the move is disallowed.
pub async fn transition(
    pool: &PgPool,
    position: &Position,
    new_state: PositionState,
    new_step: Option<WorkflowStep>,
    tx_hash: Option<&str>,
    metadata: Option<serde_json::Value>,
) -> Result<(), EngineError> {
    if !position.state.can_transition_to(&new_state) {
        return Err(EngineError::IllegalTransition {
            id:   position.id,
            from: position.state.to_string(),
            to:   new_state.to_string(),
        });
    }

    let from_str = position.state.to_string();
    let to_str   = new_state.to_string();
    let step_str = new_step.as_ref().map(|s| s.to_string());

    let mut db_tx = pool.begin().await?;

    queries::transition_state(
        &mut db_tx,
        position.id,
        position.version,
        &new_state,
        new_step.as_ref(),
    )
    .await?;

    queries::append_event(
        &mut db_tx,
        position.id,
        "state_transition",
        Some(&from_str),
        Some(&to_str),
        step_str.as_deref(),
        tx_hash,
        metadata,
    )
    .await?;

    db_tx.commit().await?;
    Ok(())
}

/// Persist that we are starting a step (idempotency checkpoint).
/// Called BEFORE submitting any on-chain transaction so that if we crash
/// between this write and tx confirmation we know what to resume.
pub async fn record_step_started(
    pool: &PgPool,
    position_id: Uuid,
    step: &WorkflowStep,
) -> Result<(), EngineError> {
    let mut db_tx = pool.begin().await?;
    queries::append_event(
        &mut db_tx,
        position_id,
        "step_started",
        None,
        None,
        Some(&step.to_string()),
        None,
        None,
    )
    .await?;
    db_tx.commit().await?;
    Ok(())
}

/// Persist that a step completed successfully (with its tx hash and new
/// financial snapshot).
pub async fn record_step_completed(
    pool: &PgPool,
    position_id: Uuid,
    step: &WorkflowStep,
    tx_hash: &str,
    collateral: Option<rust_decimal::Decimal>,
    debt: Option<rust_decimal::Decimal>,
) -> Result<(), EngineError> {
    let mut db_tx = pool.begin().await?;

    queries::update_financial_snapshot(
        &mut db_tx,
        position_id,
        collateral,
        debt,
        None,
        Some(tx_hash),
    )
    .await?;

    queries::append_event(
        &mut db_tx,
        position_id,
        "step_completed",
        None,
        None,
        Some(&step.to_string()),
        Some(tx_hash),
        None,
    )
    .await?;

    db_tx.commit().await?;
    Ok(())
}
