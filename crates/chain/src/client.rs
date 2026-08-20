/// Raw JSON-RPC client built on reqwest.
/// We use alloy only for ABI encoding/decoding (sol! macro), not for its
/// provider layer — this keeps the type system simple and gives us full
/// control over what goes over the wire (important for Guardian-signed txs).
use alloy::primitives::{Address, Bytes};
use async_trait::async_trait;
use common::{config::ChainConfig, EngineError, MorphoMarket, MorphoPosition};
use rust_decimal::Decimal;
use std::sync::Arc;

// ── Public trait ──────────────────────────────────────────────────────────────

#[async_trait]
pub trait ChainClient: Send + Sync + 'static {
    async fn get_morpho_position(
        &self,
        market_id: &str,
        user_address: &str,
    ) -> Result<MorphoPosition, EngineError>;

    async fn get_health_factor(
        &self,
        market_id: &str,
        user_address: &str,
    ) -> Result<Option<Decimal>, EngineError>;

    async fn get_morpho_market(&self, market_id: &str) -> Result<MorphoMarket, EngineError>;

    /// Submit a transaction. In production the EthClient signs with an operator
    /// key or KMS; each step calls this after building calldata.
    async fn send_transaction(&self, to: &str, calldata: Vec<u8>) -> Result<String, EngineError>;

    /// Block until tx confirmed; returns true if not reverted.
    async fn wait_for_receipt(
        &self,
        tx_hash: &str,
        timeout_secs: u64,
    ) -> Result<bool, EngineError>;

    async fn health_check(&self) -> Result<(), EngineError>;
}

// ── RPC client ────────────────────────────────────────────────────────────────

pub struct EthClient {
    pub(crate) http: reqwest::Client,
    pub(crate) rpc_url: String,
    pub(crate) morpho_address: Address,
    pub(crate) chain_id: u64,
    pub(crate) tx_timeout_secs: u64,
}

impl EthClient {
    pub fn new(cfg: &ChainConfig) -> Result<Arc<dyn ChainClient>, EngineError> {
        let morpho_address = cfg
            .morpho_address
            .parse::<Address>()
            .map_err(|e| EngineError::Config(format!("invalid Morpho address: {e}")))?;

        Ok(Arc::new(Self {
            http: reqwest::Client::new(),
            rpc_url: cfg.rpc_url.clone(),
            morpho_address,
            chain_id: cfg.chain_id,
            tx_timeout_secs: cfg.tx_timeout_secs,
        }))
    }

    /// Execute a read-only eth_call against an address.
    pub(crate) async fn eth_call(
        &self,
        to: Address,
        calldata: &[u8],
    ) -> Result<Bytes, EngineError> {
        let result = self
            .rpc_call(
                "eth_call",
                serde_json::json!([
                    {
                        "to":   format!("{to:?}"),
                        "data": format!("0x{}", hex::encode(calldata)),
                    },
                    "latest"
                ]),
            )
            .await?;

        let hex_str = result
            .as_str()
            .ok_or_else(|| EngineError::Rpc("eth_call: result is not a string".into()))?;

        let raw = alloy::primitives::hex::decode(hex_str.trim_start_matches("0x"))
            .map_err(|e| EngineError::Rpc(format!("hex decode: {e}")))?;

        Ok(Bytes::from(raw))
    }

    /// Generic JSON-RPC 2.0 call; returns the `result` field.
    pub(crate) async fn rpc_call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, EngineError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "method":  method,
            "params":  params,
            "id":      1
        });

        let resp: serde_json::Value = self
            .http
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| EngineError::Rpc(e.to_string()))?
            .json()
            .await
            .map_err(|e| EngineError::Rpc(e.to_string()))?;

        if let Some(err) = resp.get("error") {
            return Err(EngineError::Rpc(err.to_string()));
        }

        Ok(resp["result"].clone())
    }
}

// Bring hex in — alloy re-exports it.
use alloy::primitives::hex;

#[async_trait]
impl ChainClient for EthClient {
    async fn get_morpho_position(
        &self,
        market_id: &str,
        user_address: &str,
    ) -> Result<MorphoPosition, EngineError> {
        crate::morpho::get_position(self, market_id, user_address).await
    }

    async fn get_health_factor(
        &self,
        market_id: &str,
        user_address: &str,
    ) -> Result<Option<Decimal>, EngineError> {
        crate::morpho::compute_health_factor(self, market_id, user_address).await
    }

    async fn get_morpho_market(&self, market_id: &str) -> Result<MorphoMarket, EngineError> {
        crate::morpho::get_market(self, market_id).await
    }

    async fn send_transaction(&self, _to: &str, _calldata: Vec<u8>) -> Result<String, EngineError> {
        // TODO: sign with operator key or KMS, then call eth_sendRawTransaction.
        // Wire in your signer here before going to mainnet.
        Err(EngineError::Internal(
            "send_transaction: signer not configured — wire in KMS/HSM signer".into(),
        ))
    }

    async fn wait_for_receipt(
        &self,
        tx_hash: &str,
        timeout_secs: u64,
    ) -> Result<bool, EngineError> {
        use std::time::Duration;
        use tokio::time;

        time::timeout(Duration::from_secs(timeout_secs), async {
            loop {
                let result = self
                    .rpc_call(
                        "eth_getTransactionReceipt",
                        serde_json::json!([tx_hash]),
                    )
                    .await?;

                if result.is_null() {
                    time::sleep(Duration::from_secs(3)).await;
                    continue;
                }

                // EIP-658: status "0x1" = success, "0x0" = reverted.
                let status = result
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("0x0");

                return Ok(status == "0x1");
            }
        })
        .await
        .map_err(|_| EngineError::TxTimeout { seconds: timeout_secs })?
    }

    async fn health_check(&self) -> Result<(), EngineError> {
        let result = self
            .rpc_call("eth_chainId", serde_json::json!([]))
            .await?;

        let chain_id_hex = result
            .as_str()
            .ok_or_else(|| EngineError::Rpc("health_check: unexpected chain id format".into()))?;

        let chain_id = u64::from_str_radix(chain_id_hex.trim_start_matches("0x"), 16)
            .map_err(|e| EngineError::Rpc(format!("parse chain id: {e}")))?;

        if chain_id != self.chain_id {
            return Err(EngineError::Config(format!(
                "chain ID mismatch: expected {}, got {chain_id}",
                self.chain_id
            )));
        }
        Ok(())
    }
}

