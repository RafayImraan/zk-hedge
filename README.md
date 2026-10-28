# ZK-Hedge

ZK-Hedge is a cross-chain privacy protocol demo monorepo containing Solidity
escrow contracts, an Axum Rust relayer, and a Next.js dashboard.

## Repository layout

- `contracts/` - Foundry Solidity project for Arbitrum Sepolia (chain ID `421614`).
- `relayer/` - Axum intent API, Zcash Testnet Lightwalletd payment verifier,
  Arbitrum Sepolia escrow-log verifier, and an in-process FROST-Ed25519 demo signer.
- `frontend/` - Next.js App Router dashboard styled with Tailwind CSS.

Copy `.env.example` to `.env` and configure the RPC and escrow address values.

## Contracts

Install Foundry and run `forge test` from `contracts/`. The escrow emits
`SwapExecuted(bytes32,address,uint256)` with indexed intent hash and recipient.

## Relayer

Install Rust with rustup and a working platform C++ linker (Visual Studio Build
Tools with the C++ workload on Windows). Configure `ARBITRUM_SEPOLIA_RPC_URL`,
`SHIELDED_SOLVER_ESCROW_ADDRESS`, and `ZCASH_LIGHTWALLETD_URL`, then run
`cargo run` from `relayer/`. It listens on `127.0.0.1:8080`.

- `POST /api/v1/intent` validates the signed commitment and queues verification.
- `GET /api/v1/status/:intent_hash` returns the current pipeline stage.
- Set `ARBITRUM_SEPOLIA_WS_URL` to observe live `SwapExecuted` logs.

The dashboard signs the intent using a Testnet unified full viewing key exported
from the user's Zcash wallet. Submit a mined Zcash transaction ID that paid the
corresponding shielded address and the Arbitrum Sepolia `fulfillSwap` transaction
hash. The relayer checks the Zcash Testnet network, confirmations, and decrypts
shielded outputs for the requested amount before accepting the escrow event.
Never provide a seed phrase or spending key. A full viewing key reveals viewing
information to the relayer and Lightwalletd; use a trusted/self-hosted endpoint.

## Frontend

Use Node.js 20.9 or newer. From `frontend/`, run `npm install` and then
`npm run dev`. The dashboard proxies intent and status requests to the Rust
service and polls the stage every two seconds.

## Demo scope

The user broadcasts the Zcash transaction from their own wallet; the relayer
verifies it but does not custody funds or broadcast spends. FROST shares are
generated and used in one process for the demo; they are not distributed or
persisted, so this remains a hackathon demonstration rather than production
threshold custody.
