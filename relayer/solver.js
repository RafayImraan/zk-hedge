#!/usr/bin/env node
// Relayer-invoked demo solver. It refuses to broadcast until the source-chain
// payment for this exact intent has been verified.
const { spawn } = require("node:child_process");
const path = require("node:path");
const fs = require("node:fs/promises");

const root = path.resolve(__dirname, "..");
try { process.loadEnvFile(path.join(root, ".env")); } catch { /* .env is optional when variables are already exported */ }

const [intentHash, recipient] = process.argv.slice(2);
const fail = (message) => { console.error(message); process.exit(1); };
const solverPrivateKey = process.env.SOLVER_PRIVATE_KEY || process.env.DEPLOYER_PRIVATE_KEY;

if (!/^0x[0-9a-fA-F]{64}$/.test(intentHash || "")) fail("Usage: node relayer/solver.js <intent_hash> <EVM_recipient>");
if (!/^0x[0-9a-fA-F]{40}$/.test(recipient || "")) fail("Provide the intent's 0x EVM recipient address.");
if (!solverPrivateKey || !process.env.ARBITRUM_SEPOLIA_RPC_URL || !process.env.SHIELDED_SOLVER_ESCROW_ADDRESS) {
  fail("Configure the solver/deployer test key, Arbitrum Sepolia RPC URL, and escrow address in the relayer environment.");
}

async function main() {
  const api = (process.env.RELAYER_API_URL || "http://127.0.0.1:8080").replace(/\/+$/, "");
  const response = await fetch(`${api}/api/v1/status/${encodeURIComponent(intentHash)}`);
  if (!response.ok) fail(`Relayer status check failed (HTTP ${response.status}); no transaction was broadcast.`);
  const record = await response.json();
  if (record.intent_hash?.toLowerCase() !== intentHash.toLowerCase() || record.stage !== "TAZShielded") {
    fail(`Relayer has not verified this intent's Zcash payment (stage: ${record.stage || "unknown"}); no transaction was broadcast.`);
  }
  if (record.recipient_address?.toLowerCase() !== recipient.toLowerCase()) {
    fail("EVM recipient does not match the signed intent; no transaction was broadcast.");
  }
  if (record.error) fail(`Relayer reports a verification error; no transaction was broadcast.`);
  if (!/^(0x)?[0-9a-fA-F]{64}$/.test(record.zcash_txid || "")) {
    fail("Relayer record has no valid confirmed Zcash TXID; no transaction was broadcast.");
  }

  console.log(`Source payment verified. Intent: ${intentHash}`);
  console.log(`Zcash TXID: ${record.zcash_txid}`);
  console.log("Starting the Arbitrum Sepolia demo-token escrow and payout. This broadcasts transactions and spends solver test ETH.");
  const env = { ...process.env, SOLVER_PRIVATE_KEY: solverPrivateKey, INTENT_HASH: intentHash, DEMO_EVM_RECIPIENT: recipient };
  const forge = process.platform === "win32" ? "forge.exe" : "forge";
  const result = spawn(forge, [
    "script", "script/RunDemoSwap.s.sol:RunDemoSwap",
    "--rpc-url", process.env.ARBITRUM_SEPOLIA_RPC_URL,
    "--broadcast", "--chain", "421614",
  ], { cwd: path.join(root, "contracts"), env, stdio: ["ignore", "pipe", "pipe"] });
  result.stdout.on("data", (chunk) => process.stdout.write(chunk));
  result.stderr.on("data", (chunk) => process.stderr.write(chunk));
  result.on("error", (error) => fail(`Could not start Foundry: ${error.message}`));
  result.on("close", async (code) => {
    if (code !== 0) process.exit(code ?? 1);
    try {
      const broadcastPath = path.join(root, "contracts", "broadcast", "RunDemoSwap.s.sol", "421614", "run-latest.json");
      const broadcast = JSON.parse(await fs.readFile(broadcastPath, "utf8"));
      const payout = broadcast.transactions?.find((tx) => /^fulfillSwap\(/.test(tx.function || ""));
      if (!/^0x[0-9a-fA-F]{64}$/.test(payout?.hash || "")) fail("Foundry completed but the payout transaction hash was not found.");
      console.log(`SOLVER_TX_HASH=${payout.hash}`);
    } catch (error) {
      fail(`Could not read the payout transaction hash: ${error.message}`);
    }
  });
}

main().catch((error) => fail(`Solver preflight failed: ${error.message}`));
