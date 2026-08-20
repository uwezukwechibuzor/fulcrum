use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ── Position state machine ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionState {
    /// Multi-step open workflow in progress.
    Opening,
    /// Healthy; monitored by the auction engine.
    Live,
    /// Collateral top-up workflow running.
    Rebalancing,
    /// Unwind workflow in progress.
    Closing,
    /// Fully unwound; terminal.
    Closed,
    /// Unrecoverable error; terminal. Manual intervention required.
    Failed,
}

impl PositionState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Closed | Self::Failed)
    }

    pub fn is_in_progress(&self) -> bool {
        matches!(self, Self::Opening | Self::Rebalancing | Self::Closing)
    }

    /// Legal transitions enforced by the state machine.
    pub fn can_transition_to(&self, next: &PositionState) -> bool {
        match (self, next) {
            (Self::Opening,     Self::Live)        => true,
            (Self::Opening,     Self::Failed)      => true,
            (Self::Live,        Self::Rebalancing) => true,
            (Self::Live,        Self::Closing)     => true,
            (Self::Rebalancing, Self::Live)        => true,
            (Self::Rebalancing, Self::Closing)     => true,
            (Self::Rebalancing, Self::Failed)      => true,
            (Self::Closing,     Self::Closed)      => true,
            (Self::Closing,     Self::Failed)      => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for PositionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{s}")
    }
}

// ── Workflow steps ────────────────────────────────────────────────────────────

/// Every persisted step is idempotent: if we crash and resume, re-running the
/// same step must be safe (check on-chain state before submitting a tx).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStep {
    // Opening
    BorrowStablecoins,
    BuyRwaToken,
    DepositCollateral,
    BorrowAgainstCollateral,
    // Rebalancing
    TopUpCollateral,
    // Closing
    RepayDebt,
    WithdrawCollateral,
    SellRwaToken,
    RepayBridgeLoan,
}

impl std::fmt::Display for WorkflowStep {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{:?}", self));
        write!(f, "{s}")
    }
}

// ── Core domain types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    /// User wallet that owns this position.
    pub owner_address: String,
    /// RWA token address (e.g. Centrifuge pool token).
    pub rwa_token: String,
    /// Morpho market address.
    pub facility: String,
    /// Morpho market ID (bytes32 hex, uniquely identifies the market params).
    pub market_id: String,
    pub target_leverage: Decimal,
    pub state: PositionState,
    pub current_step: Option<WorkflowStep>,
    /// RWA token amount deposited as collateral (18-decimal units).
    pub collateral_amount: Option<Decimal>,
    /// Stablecoin debt outstanding (18-decimal units).
    pub debt_amount: Option<Decimal>,
    /// Current health factor; None until position is Live.
    pub health_factor: Option<Decimal>,
    /// Most recently submitted on-chain tx hash.
    pub last_tx_hash: Option<String>,
    pub error_message: Option<String>,
    /// Incremented on every write; used for optimistic concurrency.
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPositionParams {
    pub owner_address: String,
    pub rwa_token: String,
    pub facility: String,
    pub market_id: String,
    pub target_leverage: Decimal,
    /// User's initial RWA token collateral amount.
    pub initial_collateral: Decimal,
}

/// All commands that flow through the internal command bus.
/// Both the HTTP API (user intent) and the auction engine (automated triggers)
/// send commands here; the orchestrator is the sole executor.
#[derive(Debug, Clone)]
pub enum Command {
    OpenPosition(OpenPositionParams),
    ClosePosition   { position_id: Uuid },
    RebalancePosition { position_id: Uuid },
    /// Emitted at startup for positions stuck mid-workflow after a crash.
    ResumePosition  { position_id: Uuid },
}

// ── On-chain state snapshots ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MorphoPosition {
    pub supply_shares:  u128,
    pub borrow_shares:  u128,
    pub collateral:     u128,
}

#[derive(Debug, Clone)]
pub struct MorphoMarket {
    pub total_supply_assets: u128,
    pub total_supply_shares: u128,
    pub total_borrow_assets: u128,
    pub total_borrow_shares: u128,
    pub lltv: u128,
    /// Oracle price scaled to 1e36.
    pub oracle_price: u128,
}

// ── Auction engine thresholds ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct HealthThresholds {
    /// Below this → trigger rebalance (add collateral).
    pub rebalance_below: Decimal,
    /// Below this → trigger forced close (emergency unwind).
    pub close_below: Decimal,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            rebalance_below: Decimal::new(115, 2), // 1.15
            close_below:     Decimal::new(105, 2), // 1.05
        }
    }
}
