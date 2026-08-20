use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use common::{Command, OpenPositionParams};
use db::queries;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{errors::ApiError, state::AppState};

// ── Request / response types ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct OpenPositionRequest {
    pub owner_address:     String,
    pub rwa_token:         String,
    pub facility:          String,
    pub market_id:         String,
    pub target_leverage:   rust_decimal::Decimal,
    pub initial_collateral: rust_decimal::Decimal,
}

#[derive(Debug, Serialize)]
pub struct PositionResponse {
    pub id:               Uuid,
    pub owner_address:    String,
    pub rwa_token:        String,
    pub facility:         String,
    pub market_id:        String,
    pub target_leverage:  rust_decimal::Decimal,
    pub state:            String,
    pub current_step:     Option<String>,
    pub collateral_amount: Option<rust_decimal::Decimal>,
    pub debt_amount:       Option<rust_decimal::Decimal>,
    pub health_factor:     Option<rust_decimal::Decimal>,
    pub last_tx_hash:      Option<String>,
    pub error_message:     Option<String>,
    pub created_at:        chrono::DateTime<chrono::Utc>,
    pub updated_at:        chrono::DateTime<chrono::Utc>,
}

impl From<common::Position> for PositionResponse {
    fn from(p: common::Position) -> Self {
        Self {
            id:               p.id,
            owner_address:    p.owner_address,
            rwa_token:        p.rwa_token,
            facility:         p.facility,
            market_id:        p.market_id,
            target_leverage:  p.target_leverage,
            state:            p.state.to_string(),
            current_step:     p.current_step.map(|s| s.to_string()),
            collateral_amount: p.collateral_amount,
            debt_amount:       p.debt_amount,
            health_factor:     p.health_factor,
            last_tx_hash:      p.last_tx_hash,
            error_message:     p.error_message,
            created_at:        p.created_at,
            updated_at:        p.updated_at,
        }
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// POST /positions — open a new leveraged position.
/// Returns 202 Accepted immediately; poll GET /positions/:id for status.
pub async fn open_position(
    State(state): State<AppState>,
    Json(req): Json<OpenPositionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    // Basic validation
    if req.target_leverage <= rust_decimal::Decimal::ONE {
        return Err(ApiError(common::EngineError::Config(
            "target_leverage must be > 1".into(),
        )));
    }
    if req.initial_collateral <= rust_decimal::Decimal::ZERO {
        return Err(ApiError(common::EngineError::Config(
            "initial_collateral must be > 0".into(),
        )));
    }

    let params = OpenPositionParams {
        owner_address:     req.owner_address,
        rwa_token:         req.rwa_token,
        facility:          req.facility,
        market_id:         req.market_id,
        target_leverage:   req.target_leverage,
        initial_collateral: req.initial_collateral,
    };

    state
        .orchestrator
        .send(Command::OpenPosition(params))
        .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "status": "accepted", "message": "position opening in progress" })),
    ))
}

/// GET /positions/:id — fetch position state.
pub async fn get_position(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PositionResponse>, ApiError> {
    let position = queries::get_position(&state.pool, id).await?;
    Ok(Json(position.into()))
}

/// GET /positions?owner=0x... — list positions for an owner.
pub async fn list_positions(
    State(state): State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<PositionResponse>>, ApiError> {
    let owner = params
        .get("owner")
        .cloned()
        .unwrap_or_default();

    let positions = queries::list_positions_by_owner(&state.pool, &owner).await?;
    Ok(Json(positions.into_iter().map(Into::into).collect()))
}

/// POST /positions/:id/close — request a manual close.
pub async fn close_position(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    // Verify the position exists before sending the command.
    let position = queries::get_position(&state.pool, id).await?;

    if position.state.is_terminal() {
        return Err(ApiError(common::EngineError::AlreadyTerminal(id)));
    }

    state
        .orchestrator
        .send(Command::ClosePosition { position_id: id })
        .await?;

    Ok(StatusCode::ACCEPTED)
}
