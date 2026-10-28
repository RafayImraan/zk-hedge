# ZK-Hedge relayer

Rust service scaffold for intent routing and Zebra/Lightwalletd communication.
The intent schema is configured for Zcash Testnet. The Arbitrum Sepolia
verifier checks the source transaction receipt and the escrow's `SwapExecuted`
log before returning a TAZ payout authorization.

Set `ARBITRUM_SEPOLIA_RPC_URL` and `SHIELDED_SOLVER_ESCROW_ADDRESS` in the
process environment before starting the verifier. `user_signature` validation
and TAZ transaction submission remain caller responsibilities; the verifier
only confirms the EVM settlement evidence.

```sh
cargo check
cargo run
```
