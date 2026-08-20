pub mod borrow_stablecoins;
pub mod buy_rwa_token;
pub mod deposit_collateral;
pub mod borrow_against_collateral;
pub mod repay_and_close;

use chain::ChainClient;
use common::{EngineError, OpenPositionParams, Position, WorkflowStep};
use db::PgPool;
use std::sync::Arc;

/// Shared context passed into every step.
pub struct StepContext<'a> {
    pub position: &'a Position,
    pub params:   &'a OpenPositionParams,
    pub chain:    Arc<dyn ChainClient>,
    pub pool:     PgPool,
    pub tx_timeout_secs: u64,
}

/// Result of a successful step execution.
pub struct StepOutput {
    pub tx_hash:  String,
    pub collateral_delta: Option<rust_decimal::Decimal>,
    pub debt_delta:       Option<rust_decimal::Decimal>,
}

/// Dispatch to the correct step implementation.
pub async fn execute_step(
    step: &WorkflowStep,
    ctx: &StepContext<'_>,
) -> Result<StepOutput, EngineError> {
    match step {
        WorkflowStep::BorrowStablecoins        => borrow_stablecoins::execute(ctx).await,
        WorkflowStep::BuyRwaToken              => buy_rwa_token::execute(ctx).await,
        WorkflowStep::DepositCollateral        => deposit_collateral::execute(ctx).await,
        WorkflowStep::BorrowAgainstCollateral  => borrow_against_collateral::execute(ctx).await,
        WorkflowStep::RepayDebt                => repay_and_close::repay_debt(ctx).await,
        WorkflowStep::WithdrawCollateral       => repay_and_close::withdraw_collateral(ctx).await,
        WorkflowStep::SellRwaToken             => repay_and_close::sell_rwa_token(ctx).await,
        WorkflowStep::RepayBridgeLoan          => repay_and_close::repay_bridge_loan(ctx).await,
        WorkflowStep::TopUpCollateral          => deposit_collateral::top_up(ctx).await,
    }
}

/// The full ordered step sequence for opening a position.
pub fn opening_sequence() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep::BorrowStablecoins,
        WorkflowStep::BuyRwaToken,
        WorkflowStep::DepositCollateral,
        WorkflowStep::BorrowAgainstCollateral,
    ]
}

/// The full ordered step sequence for closing a position.
pub fn closing_sequence() -> Vec<WorkflowStep> {
    vec![
        WorkflowStep::RepayDebt,
        WorkflowStep::WithdrawCollateral,
        WorkflowStep::SellRwaToken,
        WorkflowStep::RepayBridgeLoan,
    ]
}
