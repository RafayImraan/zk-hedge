# ZK-Hedge guided Testnet demo

This flow verifies a real Zcash Testnet shielded payment and a real Arbitrum
Sepolia escrow transfer. The EVM asset is faucet-only `dUSDC`, not USDC. The
FROST signature step runs locally and does not transfer TAZ.

## Configure the local environment

In the ignored root `.env`, set these values. `ZEC_SHIELDED_ADDRESS` and
`ZEC_UFVK` must belong to the same dedicated Zcash Testnet receiving wallet.
The UFVK stays on the local relayer and must never be added to browser logs or
screenshots.

```env
ZEC_SHIELDED_ADDRESS=utest1...
ZEC_UFVK=uviewtest1...
ZCASH_LIGHTWALLETD_URL=https://your-testnet-lightwalletd-endpoint
ZCASH_REQUIRED_CONFIRMATIONS=1
SHIELDED_SOLVER_ESCROW_ADDRESS=0x...
SOLVER_PRIVATE_KEY=0x... # or use DEPLOYER_PRIVATE_KEY for the same funded test wallet
ARBITRUM_SEPOLIA_RPC_URL=https://...
```

Keep a separate Zingo sender wallet funded with enough TAZ for the payment and
fee. Sending from the same wallet to its own receiving address is a self-send,
not a meaningful cross-wallet swap.

## Start the relayer

In Git Bash from the repository root:

```bash
set -a
source .env
set +a
cd relayer
cargo run
```

Leave the terminal open. The dashboard loads only the configured receiving
address from `GET /api/zcash/account`; it does not receive the UFVK.

## Start the dashboard

In a second Git Bash terminal:

```bash
cd ~/Documents/zk
npm --prefix frontend run dev
```

Open `http://localhost:3000`, connect MetaMask on Arbitrum Sepolia, and create
the intent. The EVM recipient defaults to the connected MetaMask account. The
dashboard displays the Testnet receiving address and the exact memo
`ZK-HEDGE:v1:<intent-hash>`.

## Send and verify the Zcash payment

In Zingo PC, send the displayed TAZ amount from the funded sender wallet to the
dashboard address. Paste the dashboard memo exactly. Wait until the transaction
is mined, then paste its TXID into the dashboard and click **Submit Zcash TXID**.
The relayer checks Testnet, confirmations, the exact recipient, amount, and
intent memo. Once verified, the relayer automatically starts the solver.

## Automatic payout

No separate solver command is needed. The relayer runs the solver in the
background after source verification, broadcasts a faucet-token deployment,
escrow deposit, and payout on Arbitrum Sepolia, then verifies the payout. This
uses the configured solver/deployer test wallet and spends its test ETH. Keep
the relayer running until the dashboard shows the payout as complete.

The relayer verifies the matching `SwapExecuted` event and produces its local
FROST demonstration signature. The dashboard's final step means the demo-token
payout was verified and that local signature step completed; it does not mean
Zcash was paid out or that actual USDC was transferred.
