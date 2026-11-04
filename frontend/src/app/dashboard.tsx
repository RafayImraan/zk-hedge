"use client";

import { useEffect, useMemo, useState } from "react";
import { keccak256, toUtf8Bytes } from "ethers";

declare global {
  interface Window {
    ethereum?: {
      request: (args: { method: string; params?: unknown[] }) => Promise<any>;
      on?: (event: string, listener: (...args: any[]) => void) => void;
      removeListener?: (event: string, listener: (...args: any[]) => void) => void;
    };
  }
}

const CHAIN_ID = 421_614;
const CHAIN_HEX = "0x66eee";
const DEMO_USDC_AMOUNT = 25_000_000;
const STAGES = [
  { title: "Intent", detail: "Sign in wallet" },
  { title: "Zcash payment", detail: "Confirm deposit" },
  { title: "Payout", detail: "Verify on Arbitrum" },
];
const STATUS_INDEX = {
  intentsigned: -1,
  signed: -1, intent_signed: -1,
  zcashtxidsubmitted: 0, zcash_txid_submitted: 0,
  tazshielded: 1,
  taz_shielded: 1, shielded: 1,
  solverexecuted: 2,
  solver_executed: 2, solver_executed_on_sepolia: 2, executed: 2,
  frostsettlementcomplete: 2,
  frost_settlement_complete: 2, settled: 2, complete: 2,
};

function toZatoshi(value) {
  const match = value.trim().match(/^(\d+)(?:\.(\d{0,8}))?$/);
  if (!match) throw new Error("Enter a TAZ amount with up to 8 decimal places.");
  const amount = BigInt(match[1]) * 100_000_000n + BigInt((match[2] || "").padEnd(8, "0") || "0");
  if (amount <= 0n || amount > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("TAZ amount is outside the supported range.");
  }
  return Number(amount);
}

function hexMessage(message) {
  return `0x${Array.from(new TextEncoder().encode(message), (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

function statusIndex(status) {
  return typeof status === "string" ? STATUS_INDEX[status.toLowerCase().replace(/[\s-]+/g, "_")] ?? null : null;
}

function compact(address) {
  return address ? `${address.slice(0, 6)}â€¦${address.slice(-4)}` : "Connect wallet";
}

function isArbitrumSepolia(chain) {
  try { return BigInt(chain) === BigInt(CHAIN_ID); }
  catch { return false; }
}

export default function HomePage() {
  const [amountTaz, setAmountTaz] = useState("1.0");
  const [recipient, setRecipient] = useState("");
  const [zcashRecipient, setZcashRecipient] = useState("");
  const [zcashAccountReady, setZcashAccountReady] = useState(false);
  const [arbitrumTxHash, setArbitrumTxHash] = useState("");
  const [zcashTxid, setZcashTxid] = useState("");
  const [signedRequest, setSignedRequest] = useState<any>(null);
  const [wallet, setWallet] = useState("");
  const [chainId, setChainId] = useState("");
  const [progress, setProgress] = useState(-1);
  const [intentId, setIntentId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [timelineMessage, setTimelineMessage] = useState("Connect wallet to start.");
  const [zcashVerificationPending, setZcashVerificationPending] = useState(false);
  const [solverRunning, setSolverRunning] = useState(false);
  const [copiedDepositDetails, setCopiedDepositDetails] = useState(false);

  const onTargetNetwork = isArbitrumSepolia(chainId);

  useEffect(() => {
    let active = true;
    fetch("/api/zcash/account", { cache: "no-store" })
      .then(async (response) => {
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || "Zcash Testnet account is not configured.");
        if (active) {
          setZcashRecipient(body.address || "");
          setZcashAccountReady(Boolean(body.address));
        }
      })
      .catch((loadError) => { if (active) setError(loadError?.message || "Could not load the configured Zcash account."); });
    return () => { active = false; };
  }, []);
  const sendAmount = useMemo(() => {
    try { return `${toZatoshi(amountTaz).toLocaleString()} zatoshi`; }
    catch { return "Enter a valid amount"; }
  }, [amountTaz]);

  useEffect(() => {
    const ethereum = window.ethereum;
    if (!ethereum?.request) return undefined;
    let active = true;
    const refreshWalletState = async () => {
      try {
        const [accounts, currentChain] = await Promise.all([
          ethereum.request({ method: "eth_accounts" }),
          ethereum.request({ method: "eth_chainId" }),
        ]);
        if (!active) return;
        const address = accounts?.[0] || "";
        setWallet(address);
        setChainId(currentChain);
        if (address) setRecipient((current) => current || address);
        if (address && !isArbitrumSepolia(currentChain)) setNotice("Switch your wallet to Arbitrum Sepolia.");
      } catch { /* Wallet connection is optional until the user connects. */ }
    };
    const onAccountsChanged = (accounts) => {
      const address = accounts?.[0] || "";
      setWallet(address);
      if (address) setRecipient((current) => current || address);
    };
    const onChainChanged = (nextChainId) => {
      setChainId(nextChainId);
      setNotice(isArbitrumSepolia(nextChainId) ? "" : "Switch your wallet to Arbitrum Sepolia.");
    };
    void refreshWalletState();
    ethereum.on?.("accountsChanged", onAccountsChanged);
    ethereum.on?.("chainChanged", onChainChanged);
    return () => {
      active = false;
      ethereum.removeListener?.("accountsChanged", onAccountsChanged);
      ethereum.removeListener?.("chainChanged", onChainChanged);
    };
  }, []);

  useEffect(() => {
    if (!intentId || progress >= 2) return undefined;
    let stopped = false;
    let timer;
    const poll = async () => {
      try {
        const response = await fetch(`/api/v1/status/${encodeURIComponent(intentId)}`, { cache: "no-store" });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || "Status endpoint unavailable");
        const next = statusIndex(body.stage ?? body.status);
        setSolverRunning(Boolean(body.solver_running));
        if (body.transaction_hash) setArbitrumTxHash(body.transaction_hash);
        if (next !== null) {
          setProgress((current) => Math.max(current, next));
          if (next >= 1 || body.error) setZcashVerificationPending(false);
          if (body.error) setTimelineMessage(body.error);
          else setTimelineMessage(next === 2 ? "Payout complete · demo token · no TAZ unlock" : body.solver_running ? "Zcash payment confirmed · payout processing…" : next === 1 ? "Zcash payment confirmed" : next === 0 ? "Checking Zcash payment…" : "Waiting for Zcash TXID");
        } else if (body.error) {
          setTimelineMessage(body.error);
        } else {
          setTimelineMessage("Waiting for relayer");
        }
      } catch {
        setTimelineMessage("Waiting for the local relayer status endpointâ€¦");
      }
      if (!stopped && progress < 2) timer = window.setTimeout(poll, 2000);
    };
    timer = window.setTimeout(poll, 0);
    return () => { stopped = true; window.clearTimeout(timer); };
  }, [intentId, progress]);

  async function connectWallet() {
    setError("");
    setNotice("");
    if (!window.ethereum?.request) {
      setError("No EVM wallet detected. Install a browser wallet and try again.");
      return;
    }
    try {
      const accounts = await window.ethereum.request({ method: "eth_requestAccounts" });
      const address = accounts?.[0] || "";
      const currentChain = await window.ethereum.request({ method: "eth_chainId" });
      setWallet(address);
      setChainId(currentChain);
      if (address) setRecipient((current) => current || address);
      if (!isArbitrumSepolia(currentChain)) setNotice("Switch your wallet to Arbitrum Sepolia.");
    } catch (walletError) {
      setError(walletError?.message || "Wallet connection was cancelled.");
    }
  }

  async function switchNetwork() {
    const ethereum = window.ethereum;
    if (!ethereum?.request) return;
    setError("");
    try {
      await ethereum.request({ method: "wallet_switchEthereumChain", params: [{ chainId: CHAIN_HEX }] });
      const currentChain = await ethereum.request({ method: "eth_chainId" });
      setChainId(currentChain);
      setNotice(isArbitrumSepolia(currentChain) ? "" : "MetaMask has not switched to Arbitrum Sepolia yet.");
    } catch (switchError) {
      if (switchError?.code === 4902) {
        try {
          await ethereum.request({
            method: "wallet_addEthereumChain",
            params: [{
              chainId: CHAIN_HEX,
              chainName: "Arbitrum Sepolia",
              nativeCurrency: { name: "Ether", symbol: "ETH", decimals: 18 },
              rpcUrls: ["https://sepolia-rollup.arbitrum.io/rpc"],
              blockExplorerUrls: ["https://sepolia.arbiscan.io"],
            }],
          });
          const currentChain = await ethereum.request({ method: "eth_chainId" });
          setChainId(currentChain);
          setNotice(isArbitrumSepolia(currentChain) ? "" : "Select Arbitrum Sepolia in MetaMask.");
        } catch (addError) {
          setError(addError?.message || "Could not add Arbitrum Sepolia to the wallet.");
        }
      } else {
        setError(switchError?.message || "Network switch was cancelled.");
      }
    }
  }

  async function signAndSubmit(event) {
    event.preventDefault();
    setError("");
    setNotice("");
    if (!signedRequest && (!wallet || !onTargetNetwork || !zcashAccountReady)) {
      setError("Connect MetaMask on Arbitrum Sepolia and make sure the relayer has a Zcash Testnet account configured.");
      return;
    }
    if (!/^0x[0-9a-fA-F]{40}$/.test(recipient)) {
      setError("Enter a valid Arbitrum Sepolia recipient address.");
      return;
    }
    if (!zcashRecipient.trim()) { setError("Enter a Zcash Testnet recipient address."); return; }
    setBusy(true);
    try {
      if (signedRequest) {
        const canRetryPayout = progress >= 1 && timelineMessage.startsWith("automatic payout failed:");
        if (progress >= 1 && !canRetryPayout) throw new Error("Payout runs automatically after the Zcash payment is confirmed.");
        if (!/^(0x)?[0-9a-fA-F]{64}$/.test(zcashTxid)) throw new Error("Enter the mined Zcash Testnet transaction ID.");
        setZcashVerificationPending(true);
        if (canRetryPayout) setTimelineMessage("Retrying payout…");
        const response = await fetch("/api/intent", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ...signedRequest, zcash_txid: zcashTxid, arbitrum_tx_hash: null }) });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || "The relayer rejected the transaction.");
        setProgress((current) => Math.max(current, 0));
        setZcashVerificationPending(true);
        setNotice("Payment submitted. Payout runs automatically after confirmation.");
        return;
      }
      const intent = {
        amount_taz: toZatoshi(amountTaz),
        target_chain_id: CHAIN_ID,
        recipient_address: recipient,
        amount_out: DEMO_USDC_AMOUNT,
      };
      const signedPayload = { ...intent, zcash_recipient_address: zcashRecipient, user_address: wallet };
      const intentHash = keccak256(toUtf8Bytes(JSON.stringify(signedPayload, null, 2)));
      const signature = await window.ethereum.request({
        method: "personal_sign",
        params: [hexMessage(JSON.stringify(signedPayload, null, 2)), wallet],
      });

      setIntentId("");
      setProgress(-1);
      setZcashVerificationPending(false);
      setCopiedDepositDetails(false);
      setTimelineMessage("Submitting the signed intent to the relayer.");

      const requestPayload = { intent_hash: intentHash, intent: { ...intent, user_signature: signature }, zcash_recipient_address: zcashRecipient, arbitrum_tx_hash: null, user_address: wallet };
      setSignedRequest(requestPayload);

      const response = await fetch("/api/intent", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(requestPayload),
      });
      const body = await response.json();
      if (!response.ok) throw new Error(body.error || "The local relayer rejected the intent.");

      const next = statusIndex(body.stage ?? body.status);
      if (next !== null) setProgress((current) => Math.max(current, next));
      const id = body.intent_hash ?? "";
      if (typeof id === "string" && id) {
        setIntentId(id);
        setNotice("Intent signed. Follow the Zingo send instructions, then submit its mined transaction ID.");
      } else {
        setNotice("Relayer accepted the intent, but returned no intent_id for live status updates.");
      }
    } catch (submitError) {
      if (signedRequest && progress < 1) setZcashVerificationPending(false);
      setError(submitError?.message || "Could not sign and submit the intent.");
    } finally {
      setBusy(false);
    }
  }

  async function copyDepositDetails() {
    const text = `Address: ${zcashRecipient}\nAmount: ${amountTaz} TAZ\nMemo: ZK-HEDGE:v1:${intentId}`;
    try {
      await navigator.clipboard.writeText(text);
      setCopiedDepositDetails(true);
      window.setTimeout(() => setCopiedDepositDetails(false), 2500);
    } catch {
      setError("Could not copy automatically. Select and copy the address and memo manually.");
    }
  }

  return (
    <main className="min-h-screen bg-white px-5 py-8 text-black sm:px-8">
      <div className="mx-auto max-w-5xl">
        <header className="flex items-center justify-between border-b border-black/15 pb-5">
          <div>
            <p className="text-lg font-semibold tracking-tight">ZK-Hedge</p>
            <p className="mt-1 text-sm text-black/60">Zcash → Arbitrum Sepolia</p>
          </div>
          <button className="rounded-md border border-black px-4 py-2 text-sm font-medium hover:bg-black hover:text-white" onClick={connectWallet} type="button">
            {wallet ? compact(wallet) : "Connect wallet"}
          </button>
        </header>

        <div className="grid gap-8 py-8 md:grid-cols-[1.15fr_.85fr]">
          <section>
            <h1 className="text-2xl font-semibold tracking-tight">Swap TAZ</h1>
            <form className="mt-6 space-y-5" onSubmit={signAndSubmit}>
              <div className="grid gap-4 sm:grid-cols-2">
                <div>
                  <label className="text-sm font-medium" htmlFor="amount-taz">You send</label>
                  <div className="mt-2 flex items-center rounded-md border border-black/20 px-3">
                    <input id="amount-taz" className="w-full py-3 text-base outline-none" inputMode="decimal" maxLength={32} onChange={(event) => setAmountTaz(event.target.value)} value={amountTaz} placeholder="1.0" />
                    <span className="text-sm text-black/60">TAZ</span>
                  </div>
                  <p className="mt-1 text-xs text-black/55">{sendAmount}</p>
                </div>
                <div>
                  <label className="text-sm font-medium" htmlFor="amount-usdc">You receive</label>
                  <div className="mt-2 flex items-center rounded-md border border-black/20 bg-black/[0.03] px-3 py-3">
                    <span className="flex-1">25.00</span><span className="text-sm text-black/60">dUSDC</span>
                  </div>
                  <p className="mt-1 text-xs text-black/55">Arbitrum Sepolia demo token</p>
                </div>
              </div>

              <div>
                <label className="text-sm font-medium" htmlFor="recipient">Recipient</label>
                <input id="recipient" className="mt-2 w-full rounded-md border border-black/20 px-3 py-3 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setRecipient(event.target.value)} placeholder="Connect wallet to fill address" spellCheck="false" value={recipient} />
              </div>

              {signedRequest && progress < 1 && <div className="space-y-4 rounded-lg border border-black/15 p-4">
                <h2 className="font-medium">Send Zcash payment</h2>
                <div><p className="text-xs text-black/55">To</p><p className="mt-1 break-all text-sm">{zcashRecipient || "Loading…"}</p></div>
                <div><p className="text-xs text-black/55">Memo</p><p className="mt-1 break-all font-mono text-xs">{`ZK-HEDGE:v1:${intentId}`}</p></div>
                <button className="rounded-md border border-black px-3 py-2 text-sm hover:bg-black hover:text-white" onClick={copyDepositDetails} type="button">{copiedDepositDetails ? "Copied" : "Copy deposit details"}</button>
              </div>}

              {signedRequest && progress < 1 && <div>
                <label className="text-sm font-medium" htmlFor="zcash-txid">Zcash transaction ID</label>
                <input id="zcash-txid" className="mt-2 w-full rounded-md border border-black/20 px-3 py-3 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setZcashTxid(event.target.value)} placeholder="Paste mined TXID" spellCheck="false" value={zcashTxid} />
              </div>}

              {error && <p className="rounded-md border border-black/25 p-3 text-sm" role="alert">{error}</p>}
              {notice && <div className="rounded-md bg-black/[0.04] p-3 text-sm" role="status">{notice}{!onTargetNetwork && <button className="ml-2 underline" onClick={switchNetwork} type="button">Switch network</button>}</div>}

              {!signedRequest && <button className="w-full rounded-md bg-black px-4 py-3 text-sm font-medium text-white hover:bg-black/80 disabled:cursor-not-allowed disabled:opacity-40" disabled={busy || !wallet || !onTargetNetwork || !recipient || !zcashAccountReady} type="submit">
                {busy ? "Working…" : "Create intent"}
              </button>}
              {signedRequest && (progress < 1 || timelineMessage.startsWith("automatic payout failed:")) && <button className="w-full rounded-md bg-black px-4 py-3 text-sm font-medium text-white hover:bg-black/80 disabled:cursor-not-allowed disabled:opacity-40" disabled={busy || !zcashTxid} type="submit">{busy ? "Checking…" : progress >= 1 ? "Retry payout" : "Submit Zcash TXID"}</button>}
              <p className="text-xs text-black/55">{wallet ? `${compact(wallet)} · ` : "Wallet disconnected · "}{onTargetNetwork ? "Arbitrum Sepolia" : "Switch to Arbitrum Sepolia"}</p>
            </form>
          </section>

          <section className="border-t border-black/15 pt-6 md:border-l md:border-t-0 md:pl-8 md:pt-0">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold">Status</h2>
              <span className="text-xs text-black/55">{progress >= 2 ? "Complete" : intentId ? "In progress" : "Ready"}</span>
            </div>
            <ol className="mt-6 space-y-5">
              {STAGES.map((stage, index) => {
                const done = progress >= index;
                const active = (progress + 1 === index || index === 2 && solverRunning) && progress < STAGES.length - 1;
                return <li className="flex items-center gap-3" key={stage.title}>
                  <span className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border text-xs ${done ? "border-black bg-black text-white" : "border-black/25 text-black/45"}`}>{done ? "✓" : index + 1}</span>
                  <span className={`text-sm ${done || active ? "text-black" : "text-black/45"}`}>{stage.title}</span>
                  <span className="ml-auto text-xs text-black/50">{done ? "Done" : active ? "Now" : ""}</span>
                </li>;
              })}
            </ol>
            <p className="mt-6 border-t border-black/15 pt-4 text-sm" role="status">{timelineMessage}</p>
            {(zcashVerificationPending || (busy && progress < 1 && Boolean(zcashTxid))) && <div className="mt-3 flex items-center gap-2 text-sm" role="status" aria-live="polite"><span className="h-4 w-4 animate-spin rounded-full border-2 border-black/20 border-t-black" aria-hidden="true"/>Checking Zcash payment…</div>}
            {intentId && <p className="mt-4 break-all font-mono text-xs text-black/50">Intent {intentId}</p>}
            {(zcashTxid || arbitrumTxHash) && <div className="mt-4 flex flex-col items-start gap-2 text-sm">{/^(0x)?[0-9a-fA-F]{64}$/.test(zcashTxid) && <a className="underline" href={`https://testnet.zcashexplorer.app/transactions/${zcashTxid.replace(/^0x/i, "")}`} target="_blank" rel="noreferrer">Zcash transaction ↗</a>}{/^0x[0-9a-fA-F]{64}$/.test(arbitrumTxHash) && <a className="underline" href={`https://sepolia.arbiscan.io/tx/${arbitrumTxHash}`} target="_blank" rel="noreferrer">Arbitrum transaction ↗</a>}</div>}
          </section>
        </div>
      </div>
    </main>
  );
}
