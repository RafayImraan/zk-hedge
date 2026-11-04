# ZK-Hedge

ZK-Hedge is a cross-chain privacy protocol demo monorepo containing Solidity
escrow contracts, an Axum Rust relayer, and a Next.js dashboard.

## Repository layout

- `contracts/` - Foundry Solidity project for Arbitrum Sepolia (chain ID `421614`).
- `relayer/` - Axum intent API, Zcash Testnet Lightwalletd payment verifier,
  Arbitrum Sepolia escrow-log verifier, and an in-process FROST-Ed25519 demo signer.
- `frontend/` - Next.js App Router dashboard styled with Tailwind CSS.

Copy `.env.example` to `.env`. Configure the Arbitrum RPC/escrow values and a
dedicated Zcash Testnet `ZEC_SHIELDED_ADDRESS` plus matching `ZEC_UFVK`.

## Contracts

Install Foundry and run `forge test` from `contracts/`. The escrow emits
`SwapExecuted(bytes32,address,uint256)` with indexed intent hash and recipient.

## Relayer

Install Rust with rustup and a working platform C++ linker (Visual Studio Build
Tools with the C++ workload on Windows). Configure `ARBITRUM_SEPOLIA_RPC_URL`,
`SHIELDED_SOLVER_ESCROW_ADDRESS`, `ZCASH_LIGHTWALLETD_URL`,
`ZEC_SHIELDED_ADDRESS`, and `ZEC_UFVK`, then run
`cargo run` from `relayer/`. It listens on `127.0.0.1:8080`.

- `POST /api/v1/intent` validates the signed commitment and queues verification.
- `GET /api/v1/status/:intent_hash` returns the current pipeline stage.
- Set `ARBITRUM_SEPOLIA_WS_URL` to observe live `SwapExecuted` logs.

The dashboard signs an intent with MetaMask and obtains the public Zcash
receiving address from the relayer. The matching UFVK stays server-side. The
user sends TAZ in Zingo PC, includes the exact intent memo, and submits the
mined TXID. The relayer checks Testnet, confirmations, amount, receiver, and
memo, then runs the solver and verifies the EVM demo payout automatically.
See [`DEMO_RUNBOOK.md`](DEMO_RUNBOOK.md) for the step-by-step run.

## Frontend

Use Node.js 20.9 or newer. From `frontend/`, run `npm install` and then
`npm run dev`. The dashboard proxies intent and status requests to the Rust
service and polls the stage every two seconds.

## Demo scope

The user approves the Zcash send in Zingo PC. After TXID verification, the
relayer starts Foundry in the background using its configured test wallet. The
EVM asset is faucet-only `dUSDC`, not USDC. FROST shares are generated and used
in one process for demonstration; the signature does not unlock or transfer TAZ.
