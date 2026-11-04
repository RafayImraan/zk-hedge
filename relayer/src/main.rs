pub mod frost_vault;
pub mod intent;
pub mod proof_verifier;
pub mod zcash_verifier;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap, net::SocketAddr, path::PathBuf, process::Stdio, sync::Arc, time::Duration,
};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

const STAGES: [&str; 5] = [
    "IntentSigned",
    "ZcashTxidSubmitted",
    "TAZShielded",
    "SolverExecuted",
    "FrostSettlementComplete",
];
type Shared = Arc<RwLock<HashMap<String, StatusRecord>>>;

#[derive(Clone, Serialize)]
struct StatusRecord {
    intent_hash: String,
    recipient_address: String,
    stage: String,
    zcash_txid: Option<String>,
    transaction_hash: Option<String>,
    frost_signature: Option<String>,
    error: Option<String>,
    solver_running: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IntentRequest {
    intent_hash: String,
    intent: intent::Intent,
    zcash_recipient_address: String,
    zcash_txid: Option<String>,
    arbitrum_tx_hash: Option<String>,
    user_address: String,
}

#[derive(Serialize)]
struct IntentCommitment<'a> {
    amount_taz: u64,
    target_chain_id: u64,
    recipient_address: &'a str,
    amount_out: u64,
    zcash_recipient_address: &'a str,
    user_address: &'a str,
}

async fn submit(
    State(state): State<Shared>,
    Json(body): Json<IntentRequest>,
) -> Result<(StatusCode, Json<StatusRecord>), (StatusCode, String)> {
    body.intent
        .validate()
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    if body.intent.target_chain_id != proof_verifier::ARBITRUM_SEPOLIA_CHAIN_ID {
        return Err((
            StatusCode::BAD_REQUEST,
            "target_chain_id must be Arbitrum Sepolia".into(),
        ));
    }
    if body.intent_hash.len() != 66
        || !body.intent_hash.starts_with("0x")
        || !hex::decode(&body.intent_hash[2..])
            .map(|b| b.len() == 32)
            .unwrap_or(false)
    {
        return Err((
            StatusCode::BAD_REQUEST,
            "intent_hash must be a 32-byte 0x hex value".into(),
        ));
    }
    if body.zcash_recipient_address.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "zcash_recipient_address is required".into(),
        ));
    }
    let configured_zcash_recipient = std::env::var("ZEC_SHIELDED_ADDRESS").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "ZEC_SHIELDED_ADDRESS must be configured in the relayer environment".into(),
        )
    })?;
    if configured_zcash_recipient != body.zcash_recipient_address {
        return Err((
            StatusCode::BAD_REQUEST,
            "Zcash recipient does not match the relayer's configured Testnet account".into(),
        ));
    }
    if let Some(tx_hash) = body.arbitrum_tx_hash.as_deref() {
        if tx_hash.parse::<ethers::types::H256>().is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                "arbitrum_tx_hash must be a 32-byte 0x hex value".into(),
            ));
        }
    }
    if let Some(txid) = body.zcash_txid.as_deref() {
        let txid_hex = txid.strip_prefix("0x").unwrap_or(txid);
        if txid_hex.len() != 64 || hex::decode(txid_hex).is_err() {
            return Err((
                StatusCode::BAD_REQUEST,
                "zcash_txid must be a 32-byte hexadecimal transaction ID".into(),
            ));
        }
    }
    let commitment = IntentCommitment {
        amount_taz: body.intent.amount_taz,
        target_chain_id: body.intent.target_chain_id,
        recipient_address: &body.intent.recipient_address,
        amount_out: body.intent.amount_out,
        zcash_recipient_address: &body.zcash_recipient_address,
        user_address: &body.user_address,
    };
    let commitment_json = serde_json::to_string_pretty(&commitment)
        .map_err(|error| (StatusCode::BAD_REQUEST, error.to_string()))?;
    let computed_hash = ethers::utils::keccak256(commitment_json.as_bytes());
    let submitted_hash = body.intent_hash[2..].to_ascii_lowercase();
    if hex::encode(computed_hash) != submitted_hash {
        return Err((
            StatusCode::BAD_REQUEST,
            "intent_hash does not match the signed intent fields".into(),
        ));
    }
    let signer = body
        .user_address
        .parse::<ethers::types::Address>()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "user_address must be a valid EVM address".into(),
            )
        })?;
    let signature = body
        .intent
        .user_signature
        .parse::<ethers::types::Signature>()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "user_signature is not a valid EVM signature".into(),
            )
        })?;
    let signed_message_hash = ethers::utils::hash_message(commitment_json.as_bytes());
    let recovered = signature.recover(signed_message_hash).map_err(|error| {
        (
            StatusCode::BAD_REQUEST,
            format!("user_signature recovery failed: {error}"),
        )
    })?;
    if recovered != signer {
        return Err((
            StatusCode::BAD_REQUEST,
            "user_signature does not match user_address".into(),
        ));
    }
    if let Some(txid) = body.zcash_txid.as_deref() {
        if let Some(existing) = state.read().await.get(&body.intent_hash) {
            if existing.zcash_txid.as_deref() == Some(txid)
                && existing.error.is_none()
                && existing.stage != STAGES[0]
            {
                return Ok((StatusCode::ACCEPTED, Json(existing.clone())));
            }
        }
    }
    if body.arbitrum_tx_hash.is_some() && body.zcash_txid.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            "zcash_txid is required before solver verification".into(),
        ));
    }
    // The source-chain proof must be recorded before a solver is allowed to
    // run. This status gate is also checked by the local solver wrapper.
    if body.arbitrum_tx_hash.is_some()
        && state
            .read()
            .await
            .get(&body.intent_hash)
            .is_none_or(|record| record.stage != STAGES[2])
    {
        return Err((
            StatusCode::CONFLICT,
            "Zcash Testnet payment must be verified before solver execution".into(),
        ));
    }
    let record = StatusRecord {
        intent_hash: body.intent_hash.clone(),
        recipient_address: body.intent.recipient_address.clone(),
        stage: STAGES[0].into(),
        zcash_txid: body.zcash_txid.clone(),
        transaction_hash: body.arbitrum_tx_hash.clone(),
        frost_signature: None,
        error: None,
        solver_running: false,
    };
    {
        let mut statuses = state.write().await;
        statuses
            .entry(body.intent_hash.clone())
            .and_modify(|existing| {
                existing.zcash_txid = body
                    .zcash_txid
                    .clone()
                    .or_else(|| existing.zcash_txid.clone());
                existing.transaction_hash = body
                    .arbitrum_tx_hash
                    .clone()
                    .or_else(|| existing.transaction_hash.clone());
                existing.error = None;
            })
            .or_insert_with(|| record.clone());
        if body.zcash_txid.is_some() && body.arbitrum_tx_hash.is_none() {
            if let Some(existing) = statuses.get_mut(&body.intent_hash) {
                existing.stage = STAGES[1].into();
            }
        }
    }
    if body.zcash_txid.is_some() && body.arbitrum_tx_hash.is_none() {
        let shared = state.clone();
        tokio::spawn(async move {
            tracing::info!(intent_hash = %body.intent_hash, zcash_txid = ?body.zcash_txid, "Starting Zcash Testnet payment verification");
            let result = match tokio::time::timeout(
                Duration::from_secs(60),
                verify_zcash_intent(&body),
            )
            .await
            {
                Ok(result) => result,
                Err(_) => Err(anyhow::anyhow!(
                    "Zcash Lightwalletd verification timed out after 60 seconds; check the endpoint and resubmit the same TXID"
                )),
            };
            let should_run_solver = {
                let mut statuses = shared.write().await;
                match (statuses.get_mut(&body.intent_hash), result) {
                    (Some(record), Ok(received)) => {
                        record.stage = STAGES[2].into();
                        record.error = None;
                        let should_run = !record.solver_running;
                        record.solver_running = true;
                        tracing::info!(intent_hash = %body.intent_hash, zcash_txid = ?record.zcash_txid, received_zatoshi = received, "Zcash Testnet deposit confirmed; starting automatic solver");
                        should_run
                    }
                    (Some(record), Err(error)) => {
                        tracing::error!(intent_hash = %body.intent_hash, %error, "Zcash Testnet payment verification failed");
                        record.error = Some(error.to_string());
                        false
                    }
                    (None, _) => false,
                }
            };
            if should_run_solver {
                let payout = run_automatic_solver(&body).await;
                let mut statuses = shared.write().await;
                if let Some(record) = statuses.get_mut(&body.intent_hash) {
                    record.solver_running = false;
                    match payout {
                        Ok((tx_hash, signature)) => {
                            record.transaction_hash = Some(tx_hash);
                            record.stage = STAGES[4].into();
                            record.frost_signature = Some(signature);
                            record.error = None;
                        }
                        Err(error) => {
                            tracing::error!(intent_hash = %body.intent_hash, %error, "Automatic solver failed");
                            record.error = Some(error.to_string());
                        }
                    }
                }
            }
        });
    } else if body.arbitrum_tx_hash.is_some() {
        let shared = state.clone();
        tokio::spawn(async move {
            let result = process_intent(&body, &shared).await;
            let mut statuses = shared.write().await;
            if let Some(record) = statuses.get_mut(&body.intent_hash) {
                match result {
                    Ok(signature) => {
                        record.stage = STAGES[4].into();
                        record.frost_signature = Some(signature);
                    }
                    Err(error) => record.error = Some(error.to_string()),
                }
            }
        });
    }
    Ok((StatusCode::ACCEPTED, Json(record)))
}

async fn verify_zcash_intent(body: &IntentRequest) -> anyhow::Result<u64> {
    let zcash_txid = body
        .zcash_txid
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("provide zcash_txid to verify shielded funds"))?;
    let ufvk = std::env::var("ZEC_UFVK")
        .map_err(|_| anyhow::anyhow!("ZEC_UFVK must be configured in the relayer environment"))?;
    let received = zcash_verifier::verify_shielded_payment(
        zcash_txid,
        &ufvk,
        body.intent.amount_taz,
        &body.intent_hash,
        &body.zcash_recipient_address,
    )
    .await?;
    tracing::info!(intent_hash = %body.intent_hash, zcash_txid = %zcash_txid, received_zatoshi = received, "verified mined Zcash Testnet shielded payment");
    Ok(received)
}

async fn run_automatic_solver(body: &IntentRequest) -> anyhow::Result<(String, String)> {
    let relayer_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = relayer_dir
        .parent()
        .ok_or_else(|| anyhow::anyhow!("could not locate project root"))?;
    let output = tokio::process::Command::new("node")
        .arg("relayer/solver.js")
        .arg(&body.intent_hash)
        .arg(&body.intent.recipient_address)
        .current_dir(project_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        let details = stderr.lines().last().unwrap_or("solver process failed");
        anyhow::bail!("automatic payout failed: {details}");
    }
    let tx_hash = stdout
        .lines()
        .find_map(|line| line.strip_prefix("SOLVER_TX_HASH="))
        .ok_or_else(|| {
            anyhow::anyhow!("solver completed but did not return a payout transaction hash")
        })?
        .trim()
        .to_owned();
    let tx_hash = tx_hash.parse::<ethers::types::H256>()?;

    let verifier = proof_verifier::ProofVerifier::from_env().await?;
    let intent_hash = body.intent_hash.parse::<ethers::types::H256>()?;
    verifier
        .verify_swap_execution(&body.intent, intent_hash, tx_hash)
        .await?;
    let signature = frost_vault::threshold_sign_intent_hash(intent_hash.as_bytes())?;
    Ok((format!("{tx_hash:#x}"), signature))
}

async fn process_intent(body: &IntentRequest, state: &Shared) -> anyhow::Result<String> {
    verify_zcash_intent(body).await?;
    let tx = body
        .arbitrum_tx_hash
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("provide arbitrum_tx_hash to verify solver execution"))?;
    let tx = tx.parse::<ethers::types::H256>()?;
    let intent_hash = body.intent_hash.parse::<ethers::types::H256>()?;
    let verifier = proof_verifier::ProofVerifier::from_env().await?;
    verifier
        .verify_swap_execution(&body.intent, intent_hash, tx)
        .await?;
    if let Some(record) = state.write().await.get_mut(&body.intent_hash) {
        record.stage = STAGES[3].into();
    }
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let signature = frost_vault::threshold_sign_intent_hash(intent_hash.as_bytes())?;
    Ok(signature)
}

async fn status(
    State(state): State<Shared>,
    Path(hash): Path<String>,
) -> Result<Json<StatusRecord>, StatusCode> {
    state
        .read()
        .await
        .get(&hash)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[derive(Serialize)]
struct ZcashAccount {
    address: String,
    network: &'static str,
}

async fn zcash_account() -> Result<Json<ZcashAccount>, (StatusCode, String)> {
    let address = std::env::var("ZEC_SHIELDED_ADDRESS").map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Zcash Testnet receiving address is not configured in the relayer".into(),
        )
    })?;
    let decoded = zcash_keys::address::UnifiedAddress::decode(
        &zcash_protocol::consensus::Network::TestNetwork,
        &address,
    )
    .map_err(|_| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            "Configured Zcash address is not a valid Testnet Unified Address".into(),
        )
    })?;
    if decoded.orchard().is_none() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "Configured Testnet address must include a shielded Orchard receiver".into(),
        ));
    }
    Ok(Json(ZcashAccount {
        address,
        network: "Zcash Testnet",
    }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // The Lightwalletd tonic transport uses Rustls 0.23. Other dependencies in
    // this binary also use Rustls, so install its crypto backend before any
    // TLS client can be constructed.
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| anyhow::anyhow!("could not install the Rustls ring crypto provider"))?;

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(network = ?intent::ZCASH_NETWORK, "Zcash Testnet verification enabled via Lightwalletd");
    let state: Shared = Arc::new(RwLock::new(HashMap::new()));
    if let (Some(ws_url), Ok(address)) = (
        std::env::var("ARBITRUM_SEPOLIA_WS_URL")
            .ok()
            .filter(|url| !url.trim().is_empty()),
        std::env::var("SHIELDED_SOLVER_ESCROW_ADDRESS"),
    ) {
        if let Ok(address) = address.parse() {
            tokio::spawn(async move {
                if let Err(error) =
                    proof_verifier::subscribe_swap_executed_logs(ws_url, address).await
                {
                    tracing::error!(%error, "WebSocket event subscription stopped");
                }
            });
        }
    }
    let app = Router::new()
        .route("/api/v1/intent", post(submit))
        .route("/api/v1/status/{intent_hash}", get(status))
        .route("/api/zcash/account", get(zcash_account))
        .with_state(state);
    let port = std::env::var("PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "ZK-Hedge relayer listening");
    axum::serve(listener, app).await?;
    Ok(())
}
