/// The auction engine: polls all live positions every `poll_interval_secs`
/// seconds, computes health factors on-chain, and sends commands to the
/// orchestrator when action is needed.
///
/// Design:
///  - One poll loop; positions are checked in parallel (tokio::spawn per batch).
///  - SKIP LOCKED in the DB query lets multiple auction-engine instances run
///    without stepping on each other.
///  - The engine does NOT execute anything directly; it only sends Commands to
///    the orchestrator, preserving the single point of execution.
use std::{sync::Arc, time::Duration};

use chain::ChainClient;
use common::{Command, EngineError, HealthThresholds, Position};
use db::{queries, PgPool};
use orchestrator::OrchestratorHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::decisions::{self, Decision};

pub struct AuctionEngine {
    pool:            PgPool,
    chain:           Arc<dyn ChainClient>,
    orchestrator:    OrchestratorHandle,
    thresholds:      HealthThresholds,
    poll_interval:   Duration,
    batch_size:      i64,
    shutdown:        CancellationToken,
}

impl AuctionEngine {
    pub fn new(
        pool:         PgPool,
        chain:        Arc<dyn ChainClient>,
        orchestrator: OrchestratorHandle,
        thresholds:   HealthThresholds,
        poll_interval_secs: u64,
        shutdown:     CancellationToken,
    ) -> Self {
        Self {
            pool,
            chain,
            orchestrator,
            thresholds,
            poll_interval: Duration::from_secs(poll_interval_secs),
            batch_size: 50,
            shutdown,
        }
    }

    pub async fn run(self) {
        info!("auction engine started (poll_interval={:?})", self.poll_interval);

        let mut interval = tokio::time::interval(self.poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(e) = self.poll_batch().await {
                        error!("auction engine poll error: {e}");
                    }
                }
                _ = self.shutdown.cancelled() => {
                    info!("auction engine shutting down");
                    break;
                }
            }
        }
    }

    async fn poll_batch(&self) -> Result<(), EngineError> {
        let positions =
            queries::get_live_positions_for_monitoring(&self.pool, self.batch_size).await?;

        if positions.is_empty() {
            return Ok(());
        }

        info!("auction engine: checking {} live positions", positions.len());

        let mut handles = vec![];

        for position in positions {
            let chain        = self.chain.clone();
            let orchestrator = self.orchestrator.clone();
            let thresholds   = self.thresholds.clone();

            handles.push(tokio::spawn(async move {
                check_position(position, chain, orchestrator, thresholds).await;
            }));
        }

        for h in handles {
            if let Err(e) = h.await {
                error!("position check task panicked: {e}");
            }
        }

        Ok(())
    }
}

async fn check_position(
    position:     Position,
    chain:        Arc<dyn ChainClient>,
    orchestrator: OrchestratorHandle,
    thresholds:   HealthThresholds,
) {
    let hf_result = chain
        .get_health_factor(&position.market_id, &position.owner_address)
        .await;

    let health_factor = match hf_result {
        Ok(hf) => hf,
        Err(e) => {
            warn!(position_id = %position.id, "failed to read health factor: {e}");
            return;
        }
    };

    let decision = decisions::evaluate(&position, health_factor, &thresholds);

    match decision {
        Decision::Hold => {
            // Healthy — do nothing.
        }
        Decision::Rebalance => {
            info!(
                position_id = %position.id,
                health_factor = ?health_factor,
                "health factor low — triggering rebalance"
            );
            if let Err(e) = orchestrator
                .send(Command::RebalancePosition { position_id: position.id })
                .await
            {
                error!(position_id = %position.id, "failed to send rebalance command: {e}");
            }
        }
        Decision::ForceClose => {
            warn!(
                position_id = %position.id,
                health_factor = ?health_factor,
                "health factor critical — triggering force close"
            );
            if let Err(e) = orchestrator
                .send(Command::ClosePosition { position_id: position.id })
                .await
            {
                error!(position_id = %position.id, "failed to send close command: {e}");
            }
        }
    }
}
