use serde::Deserialize;

/// Loaded once at startup from environment variables (and optionally a .env
/// file). All fields are validated before the server starts accepting traffic.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
    pub chain: ChainConfig,
    pub engine: EngineConfig,
    pub api: ApiConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    /// Full PostgreSQL connection string.
    /// e.g. postgres://user:password@localhost:5432/fulcrum
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "postgres://postgres:postgres@localhost:5432/fulcrum".into(),
            max_connections: 20,
            min_connections: 2,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainConfig {
    /// HTTP RPC endpoint (Alchemy / Infura / private node).
    pub rpc_url: String,
    /// WS endpoint for subscriptions (optional; falls back to polling).
    pub ws_url: Option<String>,
    /// Chain ID — validated against the connected node at startup.
    pub chain_id: u64,
    /// Guardian HTTP URL.
    pub guardian_url: String,
    /// Morpho core contract address.
    pub morpho_address: String,
    /// How long to wait for a transaction receipt (seconds).
    pub tx_timeout_secs: u64,
    /// Max gas price in gwei before refusing to submit.
    pub max_gas_gwei: u64,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://localhost:8545".into(),
            ws_url: None,
            chain_id: 1,
            guardian_url: "http://localhost:3000".into(),
            morpho_address: "0xBBBBBbbBBb9cC5e90e3b3Af64bdAF62C37EEFFc".into(),
            tx_timeout_secs: 120,
            max_gas_gwei: 200,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EngineConfig {
    /// How often the auction engine polls on-chain health factors (seconds).
    pub poll_interval_secs: u64,
    /// Max retries for a failed step before marking position Failed.
    pub max_step_retries: u32,
    /// Base delay for exponential back-off (milliseconds).
    pub retry_base_ms: u64,
    pub health_thresholds: HealthThresholdConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HealthThresholdConfig {
    pub rebalance_below: f64,
    pub close_below: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            max_step_retries: 5,
            retry_base_ms: 500,
            health_thresholds: HealthThresholdConfig {
                rebalance_below: 1.15,
                close_below: 1.05,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiConfig {
    pub host: String,
    pub port: u16,
    /// Bearer token for API authentication.
    pub api_key: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".into(),
            port: 8080,
            api_key: "changeme".into(),
        }
    }
}

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let _ = dotenvy::dotenv(); // ignore error if .env absent

        config::Config::builder()
            .set_default("database.url", DatabaseConfig::default().url)?
            .set_default("database.max_connections", 20u32)?
            .set_default("database.min_connections", 2u32)?
            .set_default("chain.rpc_url", ChainConfig::default().rpc_url)?
            .set_default("chain.chain_id", 1u64)?
            .set_default("chain.guardian_url", ChainConfig::default().guardian_url)?
            .set_default("chain.morpho_address", ChainConfig::default().morpho_address)?
            .set_default("chain.tx_timeout_secs", 120u64)?
            .set_default("chain.max_gas_gwei", 200u64)?
            .set_default("engine.poll_interval_secs", 30u64)?
            .set_default("engine.max_step_retries", 5u32)?
            .set_default("engine.retry_base_ms", 500u64)?
            .set_default("engine.health_thresholds.rebalance_below", 1.15f64)?
            .set_default("engine.health_thresholds.close_below", 1.05f64)?
            .set_default("api.host", "0.0.0.0")?
            .set_default("api.port", 8080u16)?
            .set_default("api.api_key", "changeme")?
            .add_source(
                config::Environment::default()
                    .separator("__")
                    .try_parsing(true),
            )
            .build()?
            .try_deserialize()
    }
}
