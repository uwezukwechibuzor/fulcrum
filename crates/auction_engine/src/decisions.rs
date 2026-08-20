use common::{HealthThresholds, Position};
use rust_decimal::Decimal;

/// What the auction engine should do with a position based on current
/// on-chain health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Position is healthy; no action needed.
    Hold,
    /// Health factor is approaching the danger zone; add collateral.
    Rebalance,
    /// Health factor is critically low; force-close immediately.
    ForceClose,
}

/// Pure function — no I/O, easy to unit-test.
pub fn evaluate(
    position: &Position,
    current_health_factor: Option<Decimal>,
    thresholds: &HealthThresholds,
) -> Decision {
    let hf = match current_health_factor {
        Some(hf) => hf,
        None => return Decision::Hold, // no debt → nothing to do
    };

    if hf < thresholds.close_below {
        Decision::ForceClose
    } else if hf < thresholds.rebalance_below {
        Decision::Rebalance
    } else {
        Decision::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn dummy_position() -> Position {
        Position {
            id:               uuid::Uuid::new_v4(),
            owner_address:    "0x0".into(),
            rwa_token:        "0x0".into(),
            facility:         "0x0".into(),
            market_id:        "0x0".into(),
            target_leverage:  dec!(2),
            state:            common::PositionState::Live,
            current_step:     None,
            collateral_amount: Some(dec!(1000)),
            debt_amount:       Some(dec!(500)),
            health_factor:    None,
            last_tx_hash:     None,
            error_message:    None,
            version:          0,
            created_at:       chrono::Utc::now(),
            updated_at:       chrono::Utc::now(),
        }
    }

    #[test]
    fn healthy_position_holds() {
        let t = HealthThresholds::default();
        let d = evaluate(&dummy_position(), Some(dec!(2.0)), &t);
        assert_eq!(d, Decision::Hold);
    }

    #[test]
    fn low_health_triggers_rebalance() {
        let t = HealthThresholds::default();
        let d = evaluate(&dummy_position(), Some(dec!(1.10)), &t);
        assert_eq!(d, Decision::Rebalance);
    }

    #[test]
    fn critical_health_triggers_force_close() {
        let t = HealthThresholds::default();
        let d = evaluate(&dummy_position(), Some(dec!(1.02)), &t);
        assert_eq!(d, Decision::ForceClose);
    }

    #[test]
    fn no_debt_holds() {
        let t = HealthThresholds::default();
        let d = evaluate(&dummy_position(), None, &t);
        assert_eq!(d, Decision::Hold);
    }
}
