use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum EngineError {
    // ── Database ─────────────────────────────────────────────────────────────
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("position {0} not found")]
    PositionNotFound(Uuid),

    /// Optimistic concurrency violation — another writer updated the row.
    #[error("position {0} was modified concurrently; retry")]
    ConcurrentModification(Uuid),

    // ── Chain ────────────────────────────────────────────────────────────────
    #[error("rpc error: {0}")]
    Rpc(String),

    #[error("transaction {hash} reverted: {reason}")]
    TxReverted { hash: String, reason: String },

    #[error("transaction confirmation timed out after {seconds}s")]
    TxTimeout { seconds: u64 },

    // ── State machine ────────────────────────────────────────────────────────
    #[error("illegal state transition from {from} to {to} for position {id}")]
    IllegalTransition { id: Uuid, from: String, to: String },

    #[error("position {0} is already in a terminal state")]
    AlreadyTerminal(Uuid),

    // ── Step execution ───────────────────────────────────────────────────────
    #[error("step {step} failed for position {id}: {reason}")]
    StepFailed { id: Uuid, step: String, reason: String },

    #[error("insufficient liquidity for position {0}")]
    InsufficientLiquidity(Uuid),

    // ── Configuration ────────────────────────────────────────────────────────
    #[error("invalid configuration: {0}")]
    Config(String),

    // ── Command channel ──────────────────────────────────────────────────────
    #[error("command channel closed unexpectedly")]
    ChannelClosed,

    // ── Catch-all ────────────────────────────────────────────────────────────
    #[error("internal error: {0}")]
    Internal(String),
}

impl EngineError {
    /// True for transient errors that are safe to retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Rpc(_) | Self::TxTimeout { .. } | Self::ConcurrentModification(_)
        )
    }

    /// True for errors that should permanently fail the position.
    pub fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::TxReverted { .. }
                | Self::InsufficientLiquidity(_)
                | Self::IllegalTransition { .. }
        )
    }
}
