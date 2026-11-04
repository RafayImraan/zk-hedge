# ZK-Hedge Project Report

**Report date:** 30 October 2026  
**Project type:** Hackathon MVP / testnet demonstration  
**Networks:** Zcash Testnet and Arbitrum Sepolia (EVM chain ID `421614`)

## 1. Executive summary

ZK-Hedge is a cross-chain swap demonstration that combines a Zcash Testnet shielded payment with an ERC-20 payout on Arbitrum Sepolia. Its monorepo contains a Solidity escrow contract, a Rust HTTP relayer, and a Next.js dashboard. The user signs an intent in an EVM wallet, supplies the identifiers for the Zcash payment and Arbitrum execution, and the relayer checks the submitted transactions before reporting progress.

The EVM escrow and the off-chain verification components are implemented. A deployment is recorded in the repository's Foundry broadcast output and its address is configured in the local environment. This report describes a testnet MVP: the interface does not itself send Zcash funds or execute solver transactions, and the FROST step is an in-process demonstration rather than distributed custody or a Zcash payout.

## 2. Goal and design

The project explores how a user could express a cross-chain intent while limiting public exposure of their Zcash activity. Zcash payment information is checked off-chain by the Rust relayer using a view-only Unified Full Viewing Key (UFVK). The escrow contract does not store Zcash addresses or keys. It uses an intent hash to associate an intent with solver-funded ERC-20 liquidity and the recipient committed for that intent.

The main components are:

| Component | Technology | Responsibility |
| --- | --- | --- |
| EVM escrow | Solidity, OpenZeppelin, Foundry | Holds solver ERC-20 funds and pays the committed recipient after solver authorization. |
| Relayer | Rust, Axum, ethers-rs, Zcash libraries | Accepts signed intents, verifies Zcash shielded outputs and the EVM escrow event, and generates a demo FROST signature. |
| Dashboard | Next.js, React, Tailwind CSS | Collects intent details, connects to MetaMask, signs/submits the intent, accepts transaction IDs, and polls status. |

## 3. User and transaction flow

1. The user connects MetaMask to Arbitrum Sepolia and enters the TAZ amount and EVM recipient. The dashboard loads the configured Testnet receiving address from the local relayer; the matching UFVK remains in relayer environment configuration.
2. The dashboard signs the intent commitment with the connected EVM wallet and submits it through the Next.js API proxy to the Rust relayer.
3. The user sends the Zcash Testnet payment from Zingo PC and enters its mined TXID in the dashboard.
4. After the relayer verifies the exact shielded receiver, amount, memo, and confirmations, a local solver wrapper checks that status and runs Foundry to call `depositIntent` and `fulfillSwap` with faucet-only `dUSDC`.
5. The relayer checks the Zcash transaction through Lightwalletd and confirms the successful EVM receipt contains a matching `SwapExecuted` event.
6. After both checks pass, the relayer generates and verifies a 2-of-3 FROST-Ed25519 signature over the intent hash and reports the final status.

## 4. Smart contract

`ShieldedSolverEscrow` is a constructorless ERC-20 escrow for individual intents. A solver calls `depositIntent(token, intentHash, recipient, amount)`. The contract checks the inputs, transfers the exact requested token amount into escrow, and records the solver, token, recipient, and amount for that hash. Fee-on-transfer deposits are rejected.

Only the depositing solver can call `fulfillSwap`. The function checks that the intent is open and that the recipient and amount match the stored commitment, transfers the ERC-20 funds, marks the intent fulfilled and `executed[intentHash] = true`, and emits `SwapExecuted(bytes32 indexed intentHash, address indexed recipient, uint256 amount)`. A solver can reclaim an unfulfilled intent with `cancelIntent`.

The deployment script checks for chain ID `421614` before broadcasting. Zcash payment verification remains off-chain; deployment does not configure or store a Zcash address or viewing key. The current local `.env` points to escrow address `0x6Ab2536069D7097f4711D5e361cb2358C6248210`. Foundry's latest broadcast record lists deployment transaction `0xebdd83981d1547213c9a79ec04d7aabf299b3ba00cf26ba57d4c023a63c78ce9`.

## 5. Rust relayer

The relayer listens on `127.0.0.1:8080` by default and exposes:

- `POST /api/v1/intent` to validate the intent schema, target chain, intent hash, and recovered EVM wallet signature; it also accepts Zcash and Arbitrum transaction identifiers for verification.
- `GET /api/v1/status/:intent_hash` to retrieve the in-memory pipeline record.

The Zcash verifier connects to the configured Lightwalletd gRPC endpoint, checks that it is on Testnet, requires the transaction to be mined with the configured confirmation count, decrypts Sapling, Orchard, and Ironwood outputs with the relayer's configured Testnet UFVK, requires the decrypted receiver to match the signed Unified Address, and requires the output memo to exactly equal `ZK-HEDGE:v1:0x<64 lowercase hex digits>` for that intent. It totals matching outputs and compares received zatoshi with the requested amount. The dashboard displays the exact memo after signing. The UFVK is never part of the browser payload or UI.

The EVM verifier checks the submitted Arbitrum transaction receipt and validates the escrow's `SwapExecuted` event against the intent hash and recipient. An optional WebSocket setting can listen for live escrow events; the transaction receipt check is the path used for a submitted transaction. FROST key shares are generated within the relayer process, and the final group signature is verified before the status advances.

Status records are kept in memory, so they are lost when the relayer restarts. The local relayer API returns the configured public receiving address; it should only be exposed on a trusted local environment.

## 6. Frontend

The dashboard is a minimal black-and-white Next.js interface. It connects to MetaMask, requires Arbitrum Sepolia, loads the receiving address from the relayer, requests a wallet signature, and proxies API traffic to `RELAYER_API_URL` (local default `http://127.0.0.1:8080`). It guides the user to send through Zingo PC, accepts the resulting Zcash TXID, and polls the relayer. After source verification, a local solver wrapper checks the status before invoking Foundry for a real Arbitrum Sepolia escrow call.

The displayed receive amount is fixed at `25.00 dUSDC`, the project's faucet-only demo token; it is not USDC or a live quote. Zingo PC requires the user to approve and broadcast the Zcash payment. The local solver wrapper broadcasts the Arbitrum Sepolia demo-token escrow call after source verification. FROST remains a local signature demonstration and does not transfer TAZ.

## 7. Current implementation and verification status

- The contract, Foundry tests, deployment script, Rust server, Zcash verifier, and frontend API proxy are present in the repository.
- A Foundry broadcast record and configured escrow address are present for Arbitrum Sepolia.
- For the current integration updates, `cargo fmt --check`, `cargo check --locked`, `cargo test --locked` (5 tests), and `npm.cmd run build` completed successfully. Foundry tests could not be rerun in this Windows environment because the installed Foundry binary panicked while detecting the home directory; deployment script/test execution therefore still needs local Foundry verification.
- Local service availability has previously been observed for the Rust HTTP server. A request to an unknown status hash returned HTTP 404, which indicates the route was reachable but no matching intent record existed.
- The configured public Lightwalletd endpoint and an actual Zcash payment verification have not been confirmed as working end-to-end in this report session.
- The `ARBITRUM_SEPOLIA_WS_URL` setting is optional; a blank value disables the optional WebSocket listener.
- `DEMO_RUNBOOK.md` documents the guided real Testnet path. The EVM payout uses a faucet-only demo ERC-20, not actual USDC.

## 8. Scope and limitations

This is a testnet demonstration, not a production bridge or privacy guarantee. The relayer is trusted to perform the off-chain checks correctly. The memo binds the shielded output to the signed intent at the relayer layer; the Solidity contract does not independently verify Zcash proofs or the memo. No Zcash payout is created by the FROST routine. The FROST demonstration creates shares locally in one process rather than distributing them among independent parties. Intent statuses are in-memory and are not durable. The project also depends on external wallet, RPC, Lightwalletd, solver, and test-token availability.

For a complete run, the operator needs a working Lightwalletd Testnet endpoint, a matching Testnet UFVK and shielded recipient configured in the local relayer, a mined memo-bound Zcash Testnet payment, and a solver with Arbitrum Sepolia test ETH. The solver deploys faucet-only `dUSDC` and executes `depositIntent`/`fulfillSwap`; it is not a live USDC payout.

## 9. Conclusion

ZK-Hedge demonstrates wallet-signed commitments, off-chain Zcash shielded-output checks, EVM event verification, a guided user-approved payment workflow, and a local threshold-signature demonstration. A full run still depends on the configured Testnet services and an operator-approved Zingo send plus solver broadcast. It should be presented as a hackathon Testnet MVP: faucet-only `dUSDC` on Arbitrum Sepolia and simulated local FROST, with no Zcash payout.
