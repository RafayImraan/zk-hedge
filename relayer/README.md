# ZK-Hedge relayer

Rust service scaffold for intent routing and Zebra/Lightwalletd communication.
The intent schema is configured for Zcash Testnet. The Arbitrum Sepolia
verifier checks the source transaction receipt and the escrow's `SwapExecuted`
log before reporting demo settlement evidence. It does not authorize or send a
Zcash payout.

Set `ARBITRUM_SEPOLIA_RPC_URL`, `SHIELDED_SOLVER_ESCROW_ADDRESS`,
`ZEC_SHIELDED_ADDRESS`, and `ZEC_UFVK` in the local process environment before
starting the relayer. The public `GET /api/zcash/account` endpoint returns only
the configured Testnet address; the UFVK remains on the relayer and is never
included in browser payloads or logs. Keep this local API bound to loopback and
use a dedicated Testnet wallet.

For a live Zcash payment, the sender must pay the exact signed Testnet Unified
Address and put `ZK-HEDGE:v1:0x<64 lowercase hex digits>` in the shielded output memo.
The value is the 32-byte intent hash. The relayer counts only decrypted
Sapling/Orchard/Ironwood outputs whose receiver matches that address and whose memo
matches exactly. The memo is encrypted with the shielded output and is checked
off-chain; the EVM contract does not verify it.

The guided flow is: create and sign the intent, send the Zcash payment, and
submit its mined TXID. After verification, the relayer automatically runs the
solver in the background and verifies the Arbitrum payout. No manual solver
command or payout hash entry is needed. The solver uses Foundry and the
faucet-only `dUSDC` contract from `RunDemoSwap.s.sol`, not USDC. The final
FROST signature is a local demonstration; it does not unlock or transfer TAZ.

```sh
cargo check
cargo run
```
