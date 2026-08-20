use common::AppConfig;
use db::PgPool;
use orchestrator::OrchestratorHandle;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool:         PgPool,
    pub orchestrator: OrchestratorHandle,
    pub config:       Arc<AppConfig>,
}
