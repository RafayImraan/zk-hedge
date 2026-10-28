# ZK-Hedge contracts

Foundry project configured for Arbitrum Sepolia (chain ID `421614`).
`ShieldedSolverEscrow` holds an exact ERC-20 deposit for an intent, binds the
deposit to a solver and recipient, and allows that solver to fulfill or cancel
the intent. A hash cannot be reused after either outcome.

```sh
forge install OpenZeppelin/openzeppelin-contracts --no-commit
forge build
forge test
```

The current implementation rejects fee-on-transfer tokens so its recorded
escrow amount always matches the tokens received. Deploying the contract on
Arbitrum Sepolia still requires a funded deployer and a configured RPC URL.
