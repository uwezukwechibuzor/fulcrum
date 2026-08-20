use chrono::Utc;
use common::{EngineError, Position, PositionState, WorkflowStep};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

// ── Row type (private) ────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct PositionRow {
    id:                Uuid,
    owner_address:     String,
    rwa_token:         String,
    facility:          String,
    market_id:         String,
    target_leverage:   Decimal,
    state:             String,
    current_step:      Option<String>,
    collateral_amount: Option<Decimal>,
    debt_amount:       Option<Decimal>,
    health_factor:     Option<Decimal>,
    last_tx_hash:      Option<String>,
    error_message:     Option<String>,
    version:           i64,
    created_at:        chrono::DateTime<Utc>,
    updated_at:        chrono::DateTime<Utc>,
}

impl TryFrom<PositionRow> for Position {
    type Error = EngineError;

    fn try_from(r: PositionRow) -> Result<Self, Self::Error> {
        let state: PositionState = serde_json::from_value(Value::String(r.state))
            .map_err(|e| EngineError::Internal(format!("bad state in db: {e}")))?;
        let current_step: Option<WorkflowStep> = r
            .current_step
            .map(|s| {
                serde_json::from_value(Value::String(s))
                    .map_err(|e| EngineError::Internal(format!("bad step in db: {e}")))
            })
            .transpose()?;

        Ok(Position {
            id:                r.id,
            owner_address:     r.owner_address,
            rwa_token:         r.rwa_token,
            facility:          r.facility,
            market_id:         r.market_id,
            target_leverage:   r.target_leverage,
            state,
            current_step,
            collateral_amount: r.collateral_amount,
            debt_amount:       r.debt_amount,
            health_factor:     r.health_factor,
            last_tx_hash:      r.last_tx_hash,
            error_message:     r.error_message,
            version:           r.version,
            created_at:        r.created_at,
            updated_at:        r.updated_at,
        })
    }
}

// ── Reads ─────────────────────────────────────────────────────────────────────

pub async fn get_position(pool: &PgPool, id: Uuid) -> Result<Position, EngineError> {
    let row = sqlx::query_as::<_, PositionRow>(
        "SELECT * FROM positions WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(EngineError::PositionNotFound(id))?;

    row.try_into()
}

pub async fn get_live_positions_for_monitoring(
    pool: &PgPool,
    batch_size: i64,
) -> Result<Vec<Position>, EngineError> {
    let rows = sqlx::query_as::<_, PositionRow>(
        r#"
        SELECT *
        FROM   positions
        WHERE  state = 'live'
        ORDER  BY updated_at ASC
        LIMIT  $1
        "#,
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|r| r.try_into()).collect()
}

pub async fn get_in_progress_positions(pool: &PgPool) -> Result<Vec<Position>, EngineError> {
    let rows = sqlx::query_as::<_, PositionRow>(
        "SELECT * FROM positions WHERE state IN ('opening','rebalancing','closing')",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|r| r.try_into()).collect()
}

pub async fn list_positions_by_owner(
    pool: &PgPool,
    owner: &str,
) -> Result<Vec<Position>, EngineError> {
    let rows = sqlx::query_as::<_, PositionRow>(
        "SELECT * FROM positions WHERE owner_address = $1 ORDER BY created_at DESC",
    )
    .bind(owner)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|r| r.try_into()).collect()
}

// ── Writes ────────────────────────────────────────────────────────────────────

pub async fn insert_position(
    pool: &PgPool,
    id: Uuid,
    owner_address: &str,
    rwa_token: &str,
    facility: &str,
    market_id: &str,
    target_leverage: Decimal,
) -> Result<Position, EngineError> {
    let row = sqlx::query_as::<_, PositionRow>(
        r#"
        INSERT INTO positions
            (id, owner_address, rwa_token, facility, market_id, target_leverage, state)
        VALUES ($1, $2, $3, $4, $5, $6, 'opening')
        RETURNING *
        "#,
    )
    .bind(id)
    .bind(owner_address)
    .bind(rwa_token)
    .bind(facility)
    .bind(market_id)
    .bind(target_leverage)
    .fetch_one(pool)
    .await?;

    row.try_into()
}

/// Transition state + increment version atomically.
/// Returns `ConcurrentModification` if another writer changed the row first.
pub async fn transition_state<'c>(
    tx: &mut Transaction<'c, Postgres>,
    id: Uuid,
    old_version: i64,
    new_state: &PositionState,
    new_step: Option<&WorkflowStep>,
) -> Result<(), EngineError> {
    let state_str = serde_json::to_value(new_state)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| format!("{new_state:?}"));

    let step_str: Option<String> = new_step.map(|s| {
        serde_json::to_value(s)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{s:?}"))
    });

    let rows = sqlx::query(
        r#"
        UPDATE positions
        SET    state        = $1,
               current_step = $2,
               version      = version + 1
        WHERE  id      = $3
        AND    version = $4
        "#,
    )
    .bind(&state_str)
    .bind(&step_str)
    .bind(id)
    .bind(old_version)
    .execute(&mut **tx)
    .await?;

    if rows.rows_affected() == 0 {
        return Err(EngineError::ConcurrentModification(id));
    }
    Ok(())
}

pub async fn update_financial_snapshot<'c>(
    tx: &mut Transaction<'c, Postgres>,
    id: Uuid,
    collateral_amount: Option<Decimal>,
    debt_amount: Option<Decimal>,
    health_factor: Option<Decimal>,
    last_tx_hash: Option<&str>,
) -> Result<(), EngineError> {
    sqlx::query(
        r#"
        UPDATE positions
        SET    collateral_amount = $1,
               debt_amount       = $2,
               health_factor     = $3,
               last_tx_hash      = $4
        WHERE  id = $5
        "#,
    )
    .bind(collateral_amount)
    .bind(debt_amount)
    .bind(health_factor)
    .bind(last_tx_hash)
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    reason: &str,
) -> Result<(), EngineError> {
    sqlx::query(
        r#"
        UPDATE positions
        SET    state         = 'failed',
               current_step  = NULL,
               error_message = $1,
               version       = version + 1
        WHERE  id = $2
        "#,
    )
    .bind(reason)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Audit trail ───────────────────────────────────────────────────────────────

pub async fn append_event<'c>(
    tx: &mut Transaction<'c, Postgres>,
    position_id: Uuid,
    event_type: &str,
    from_state: Option<&str>,
    to_state: Option<&str>,
    step: Option<&str>,
    tx_hash: Option<&str>,
    metadata: Option<Value>,
) -> Result<(), EngineError> {
    sqlx::query(
        r#"
        INSERT INTO workflow_events
            (position_id, event_type, from_state, to_state, step, tx_hash, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
    )
    .bind(position_id)
    .bind(event_type)
    .bind(from_state)
    .bind(to_state)
    .bind(step)
    .bind(tx_hash)
    .bind(metadata)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
