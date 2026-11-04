# ZK-Hedge dashboard

The dashboard signs an intent with MetaMask on Arbitrum Sepolia. It loads the
configured Zcash Testnet receiving address from the local Rust relayer. The
matching UFVK stays in the relayer environment and is never sent to the
browser.

The user sends TAZ through Zingo PC, including the exact intent memo shown by
the dashboard, then submits the mined Zcash TXID. The relayer verifies the
source payment and automatically runs and verifies the solver payout. The
user does not need to run a terminal command or paste an Arbitrum transaction
hash.

The payout asset is faucet-only `dUSDC`, not USDC. FROST is a local signature
demonstration; it does not unlock or transfer TAZ.

```sh
npm install
npm run dev
```
