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
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

const STAGES: [&str; 4] = [
    "IntentSigned",
    "TAZShielded",
    "SolverExecuted",
    "FrostSettlementComplete",
];
type Shared = Arc<RwLock<HashMap<String, StatusRecord>>>;

#[derive(Clone, Serialize)]
struct StatusRecord {
    intent_hash: String,
    stage: String,
    zcash_txid: Option<String>,
    transaction_hash: Option<String>,
    frost_signature: Option<String>,
    error: Option<String>,
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
    ephemeral_viewing_key: &'a str,
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
    let Some(tx_hash) = body.arbitrum_tx_hash.as_deref() else {
        return Err((
            StatusCode::BAD_REQUEST,
            "arbitrum_tx_hash is required".into(),
        ));
    };
    if tx_hash.parse::<ethers::types::H256>().is_err() {
        return Err((
            StatusCode::BAD_REQUEST,
            "arbitrum_tx_hash must be a 32-byte 0x hex value".into(),
        ));
    }
    let commitment = IntentCommitment {
        amount_taz: body.intent.amount_taz,
        target_chain_id: body.intent.target_chain_id,
        recipient_address: &body.intent.recipient_address,
        ephemeral_viewing_key: &body.intent.ephemeral_viewing_key,
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
    if body.arbitrum_tx_hash.is_some() && body.zcash_txid.is_none() {
        return Err((StatusCode::BAD_REQUEST, "zcash_txid is required before solver verification".into()));
    }
    let record = StatusRecord {
        intent_hash: body.intent_hash.clone(),
        stage: STAGES[0].into(),
        zcash_txid: body.zcash_txid.clone(),
        transaction_hash: body.arbitrum_tx_hash.clone(),
        frost_signature: None,
        error: None,
    };
    state
        .write()
        .await
        .insert(body.intent_hash.clone(), record.clone());
    if body.arbitrum_tx_hash.is_some() {
        let shared = state.clone();
        tokio::spawn(async move {
            let result = process_intent(&body, &shared).await;
            let mut statuses = shared.write().await;
            if let Some(record) = statuses.get_mut(&body.intent_hash) {
                match result {
                    Ok(signature) => {
                        record.stage = STAGES[3].into();
                        record.frost_signature = Some(signature);
                    }
                    Err(error) => record.error = Some(error.to_string()),
                }
            }
        });
    }
    Ok((StatusCode::ACCEPTED, Json(record)))
}

async fn process_intent(body: &IntentRequest, state: &Shared) -> anyhow::Result<String> {
    let zcash_txid = body.zcash_txid.as_deref().ok_or_else(|| anyhow::anyhow!("provide zcash_txid to verify shielded funds"))?;
    let received = zcash_verifier::verify_shielded_payment(zcash_txid, &body.intent.ephemeral_viewing_key, body.intent.amount_taz).await?;
    tracing::info!(intent_hash = %body.intent_hash, received_zatoshi = received, "verified mined Zcash Testnet shielded payment");
    if let Some(record) = state.write().await.get_mut(&body.intent_hash) { record.stage = STAGES[1].into(); }
    let tx = body
        .arbitrum_tx_hash
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("provide arbitrum_tx_hash to verify solver execution"))?;
    let tx = tx.parse::<ethers::types::H256>()?;
    let intent_hash = body.intent_hash.parse::<ethers::types::H256>()?;
    let verifier = proof_verifier::ProofVerifier::from_env().await?;
    verifier
        .verify_swap_and_authorize_taz_payout(&body.intent, intent_hash, tx)
        .await?;
    if let Some(record) = state.write().await.get_mut(&body.intent_hash) {
        record.stage = STAGES[2].into();
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    tracing::info!(network = ?intent::ZCASH_NETWORK, "Zcash Testnet verification enabled via Lightwalletd");
    let state: Shared = Arc::new(RwLock::new(HashMap::new()));
    if let (Some(ws_url), Ok(address)) = (
        std::env::var("ARBITRUM_SEPOLIA_WS_URL").ok().filter(|url| !url.trim().is_empty()),
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
        .with_state(state);
    let address: SocketAddr = "127.0.0.1:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "ZK-Hedge relayer listening");
    axum::serve(listener, app).await?;
    Ok(())
}
