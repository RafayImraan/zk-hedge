# Arbitrum Sepolia deployment

`ShieldedSolverEscrow` has no constructor arguments. It stores per-intent EVM
escrow state when a solver calls `depositIntent`; deployment does not register
Zcash addresses, viewing keys, or token settings.

## Deploy with Foundry

Use a dedicated Arbitrum Sepolia test wallet with test ETH for gas. The private
key is required only by the deploy command. Do not commit it or paste it into
screenshots, chat, or the browser dashboard.

In PowerShell, from the repository's `contracts` directory:

```powershell
$env:ARBITRUM_SEPOLIA_RPC_URL = "https://sepolia-rollup.arbitrum.io/rpc"
$secureKey = Read-Host "Dedicated testnet deployer private key" -AsSecureString
$env:DEPLOYER_PRIVATE_KEY = [System.Net.NetworkCredential]::new("", $secureKey).Password

forge script .\script\DeployShieldedEscrow.s.sol:DeployShieldedEscrow `
  --rpc-url $env:ARBITRUM_SEPOLIA_RPC_URL `
  --private-key $env:DEPLOYER_PRIVATE_KEY `
  --broadcast `
  --chain 421614
```

The script checks the chain ID before broadcasting. Copy the deployed contract
address from Foundry's output into the relayer environment:

```env
SHIELDED_SOLVER_ESCROW_ADDRESS=0xDeployedContractAddress
```

## Zcash configuration and intent flow

`ZEC_SHIELDED_ADDRESS` and `ZEC_UFVK` are **not Solidity deployment
parameters**. The escrow contract has no Zcash storage or proof-verifier hook;
Zcash payment verification runs in the Rust relayer. The current dashboard
includes the shielded address and view-only key in the signed intent request,
and the relayer uses them to verify a mined Zcash Testnet transaction.

Do not put a UFVK on-chain or in a public Vercel variable. A full viewing key
discloses the account's transaction history. Use a dedicated Testnet account.
The dashboard requires a Testnet UFVK and its matching shielded recipient.

The cross-chain binding is the signed `intent_hash`:

1. User signs the intent containing the Zcash recipient and Arbitrum recipient.
2. The solver calls `depositIntent(token, intentHash, recipient, amount)` to
   escrow ERC-20 liquidity for that hash.
3. The relayer verifies the Zcash Testnet transaction for the requested amount.
4. The solver calls `fulfillSwap(intentHash, recipient, amount)`; the escrow
   pays the committed recipient and emits `SwapExecuted`.
5. The relayer confirms that Arbitrum Sepolia event, then performs its local
   FROST-Ed25519 demo-signing step.

There is no initial solver-to-Zcash mapping to configure at deployment. The
mapping is established per swap by `intentHash` and the `depositIntent` call.
