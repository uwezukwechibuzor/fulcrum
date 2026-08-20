use std::sync::Arc;

use auction_engine::{watcher::AuctionEngine};
use chain::EthClient;
use common::{AppConfig, HealthThresholds};
use db::connect;
use orchestrator::OrchestratorWorker;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // ── Logging ───────────────────────────────────────────────────────────────
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn".into()),
        )
        .json()
        .init();

    info!("fulcrum starting");

    // ── Config ────────────────────────────────────────────────────────────────
    let cfg = AppConfig::from_env()?;
    info!(chain_id = cfg.chain.chain_id, "configuration loaded");

    // ── Database ──────────────────────────────────────────────────────────────
    let pool = connect(&cfg.database).await?;
    info!("database connected and migrations applied");

    // ── Chain client ──────────────────────────────────────────────────────────
    let chain = EthClient::new(&cfg.chain)?;
    chain.health_check().await?;
    info!("rpc connection verified");

    // ── Graceful shutdown token ───────────────────────────────────────────────
    let shutdown = CancellationToken::new();

    // ── Orchestrator ──────────────────────────────────────────────────────────
    let (orchestrator_worker, orchestrator_handle) =
        OrchestratorWorker::new(pool.clone(), chain.clone(), &cfg, shutdown.clone());

    // ── Auction engine ────────────────────────────────────────────────────────
    let thresholds = HealthThresholds {
        rebalance_below: rust_decimal::Decimal::try_from(
            cfg.engine.health_thresholds.rebalance_below,
        )
        .unwrap(),
        close_below: rust_decimal::Decimal::try_from(
            cfg.engine.health_thresholds.close_below,
        )
        .unwrap(),
    };

    let auction_engine = AuctionEngine::new(
        pool.clone(),
        chain.clone(),
        orchestrator_handle.clone(),
        thresholds,
        cfg.engine.poll_interval_secs,
        shutdown.clone(),
    );

    // ── HTTP server ───────────────────────────────────────────────────────────
    let app_state = api::state::AppState {
        pool:         pool.clone(),
        orchestrator: orchestrator_handle,
        config:       Arc::new(cfg.clone()),
    };

    let router = api::build_router(app_state);
    let addr   = format!("{}:{}", cfg.api.host, cfg.api.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(addr = %addr, "HTTP server listening");

    // ── Spawn all services ────────────────────────────────────────────────────
    let shutdown_clone = shutdown.clone();

    tokio::spawn(async move { orchestrator_worker.run().await });
    tokio::spawn(async move { auction_engine.run().await });

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            wait_for_signal().await;
            info!("shutdown signal received");
            shutdown_clone.cancel();
        })
        .await?;

    info!("fulcrum stopped");
    Ok(())
}

async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c    => {}
        _ = terminate => {}
    }
}
