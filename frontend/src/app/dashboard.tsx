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
const STAGES = [
  { title: "Intent Signed", detail: "Wallet signature recorded" },
  { title: "TAZ Shielded", detail: "Waiting for Zcash Testnet proof" },
  { title: "Solver Executed on Sepolia", detail: "Waiting for escrow event" },
  { title: "FROST Settlement Complete", detail: "Waiting for threshold settlement" },
];
const STATUS_INDEX = {
  intentsigned: 0,
  signed: 0, intent_signed: 0, received: 0, queued: 0,
  tazshielded: 1,
  taz_shielded: 1, shielded: 1,
  solverexecuted: 2,
  solver_executed: 2, solver_executed_on_sepolia: 2, executed: 2,
  frostsettlementcomplete: 3,
  frost_settlement_complete: 3, settled: 3, complete: 3,
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

export default function HomePage() {
  const [amountTaz, setAmountTaz] = useState("1.0");
  const [recipient, setRecipient] = useState("");
  const [zcashRecipient, setZcashRecipient] = useState("");
  const [arbitrumTxHash, setArbitrumTxHash] = useState("");
  const [zcashTxid, setZcashTxid] = useState("");
  const [signedRequest, setSignedRequest] = useState<any>(null);
  const [wallet, setWallet] = useState("");
  const [chainId, setChainId] = useState("");
  const [viewingKey, setViewingKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [keyUsed, setKeyUsed] = useState(false);
  const [progress, setProgress] = useState(-1);
  const [intentId, setIntentId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [timelineMessage, setTimelineMessage] = useState("Sign an intent to begin.");

  const onTargetNetwork = chainId === CHAIN_HEX;
  const sendAmount = useMemo(() => {
    try { return `${toZatoshi(amountTaz).toLocaleString()} zatoshi`; }
    catch { return "Enter a valid amount"; }
  }, [amountTaz]);

  useEffect(() => {
    const ethereum = window.ethereum;
    if (!ethereum?.on) return undefined;
    const onAccountsChanged = (accounts) => {
      const address = accounts?.[0] || "";
      setWallet(address);
      if (address) setRecipient((current) => current || address);
    };
    const onChainChanged = (nextChainId) => setChainId(nextChainId);
    ethereum.on("accountsChanged", onAccountsChanged);
    ethereum.on("chainChanged", onChainChanged);
    return () => {
      ethereum.removeListener?.("accountsChanged", onAccountsChanged);
      ethereum.removeListener?.("chainChanged", onChainChanged);
    };
  }, []);

  useEffect(() => {
    if (!intentId || progress >= 3) return undefined;
    let stopped = false;
    let timer;
    const poll = async () => {
      try {
        const response = await fetch(`/api/v1/status/${encodeURIComponent(intentId)}`, { cache: "no-store" });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || "Status endpoint unavailable");
        const next = statusIndex(body.stage ?? body.status);
        if (body.error) setTimelineMessage(`Relayer error: ${body.error}`);
        if (next !== null) {
          setProgress((current) => Math.max(current, next));
          setTimelineMessage(next === 3 ? "FROST settlement confirmed by relayer." : "Status synced from relayer.");
        } else {
          setTimelineMessage("Intent accepted; waiting for a recognized relayer status.");
        }
      } catch {
        setTimelineMessage("Waiting for the local relayer status endpointâ€¦");
      }
      if (!stopped && progress < 3) timer = window.setTimeout(poll, 2000);
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
      if (currentChain !== CHAIN_HEX) setNotice("Switch your wallet to Arbitrum Sepolia.");
    } catch (walletError) {
      setError(walletError?.message || "Wallet connection was cancelled.");
    }
  }

  async function signAndSubmit(event) {
    event.preventDefault();
    setError("");
    setNotice("");
    if (!signedRequest && (!wallet || !onTargetNetwork || !viewingKey || keyUsed)) {
      setError("Connect a wallet on Arbitrum Sepolia and enter a Testnet viewing key.");
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
        if (!/^(0x)?[0-9a-fA-F]{64}$/.test(zcashTxid)) throw new Error("Enter the 32-byte mined Zcash Testnet transaction ID.");
        if (!/^0x[0-9a-fA-F]{64}$/.test(arbitrumTxHash)) throw new Error("Enter the 32-byte Arbitrum Sepolia transaction hash to verify.");
        const response = await fetch("/api/intent", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ ...signedRequest, zcash_txid: zcashTxid, arbitrum_tx_hash: arbitrumTxHash }) });
        const body = await response.json();
        if (!response.ok) throw new Error(body.error || "The relayer rejected the transaction hash.");
        setNotice("Transaction submitted. Waiting for the relayer to verify SwapExecuted.");
        return;
      }
      const intent = {
        amount_taz: toZatoshi(amountTaz),
        target_chain_id: CHAIN_ID,
        recipient_address: recipient,
        ephemeral_viewing_key: viewingKey,
      };
      const signedPayload = { ...intent, zcash_recipient_address: zcashRecipient, user_address: wallet };
      const intentHash = keccak256(toUtf8Bytes(JSON.stringify(signedPayload, null, 2)));
      const signature = await window.ethereum.request({
        method: "personal_sign",
        params: [hexMessage(JSON.stringify(signedPayload, null, 2)), wallet],
      });

      // The supplied read-only viewing key is included in the signed intent.
      setKeyUsed(true);
      setViewingKey("");
      setShowKey(false);
      setIntentId("");
      setProgress(0);
      setTimelineMessage("Sending signed intent to the relayer.");

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
        setNotice("Intent signed and submitted.");
      } else {
        setNotice("Relayer accepted the intent, but returned no intent_id for live status updates.");
      }
    } catch (submitError) {
      setError(submitError?.message || "Could not sign and submit the intent.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="min-h-screen bg-white px-5 py-8 text-black sm:px-8">
      <div className="mx-auto max-w-5xl">
        <header className="flex flex-wrap items-center justify-between gap-4 border-b border-black/10 pb-5">
          <div><p className="text-lg font-semibold tracking-tight">ZK-Hedge</p><p className="mt-1 text-sm text-black/60">Zcash to Arbitrum Sepolia</p></div>
          <button className="rounded-md border border-black px-4 py-2 text-sm font-medium hover:bg-black hover:text-white" onClick={connectWallet} type="button">{wallet ? compact(wallet) : "Connect wallet"}</button>
        </header>

        <div className="grid gap-8 py-8 md:grid-cols-[1.1fr_.9fr]">
          <section>
            <h1 className="text-2xl font-semibold tracking-tight">Send a swap intent</h1>
            <form className="mt-6 space-y-5" onSubmit={signAndSubmit}>
              <div className="grid gap-4 sm:grid-cols-2">
                <div><label className="text-sm font-medium" htmlFor="amount-taz">Amount to send (TAZ)</label><input id="amount-taz" className="mt-2 w-full rounded-md border border-black/20 px-3 py-2.5 text-sm outline-none focus:border-black" inputMode="decimal" maxLength={32} onChange={(event) => setAmountTaz(event.target.value)} value={amountTaz} placeholder="1.0" /><p className="mt-1 text-xs text-black/55">{sendAmount} · Zcash Testnet</p></div>
                <div><label className="text-sm font-medium" htmlFor="amount-usdc">Amount to receive (USDC)</label><input id="amount-usdc" className="mt-2 w-full rounded-md border border-black/20 bg-black/[0.03] px-3 py-2.5 text-sm" readOnly value="25.00" /><p className="mt-1 text-xs text-black/55">Arbitrum Sepolia</p></div>
              </div>
              <div><label className="text-sm font-medium" htmlFor="recipient">Arbitrum recipient address</label><input id="recipient" className="mt-2 w-full rounded-md border border-black/20 px-3 py-2.5 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setRecipient(event.target.value)} placeholder="0x…" spellCheck="false" value={recipient} /></div>
              <div><label className="text-sm font-medium" htmlFor="zcash-recipient">Zcash Testnet shielded address</label><input id="zcash-recipient" className="mt-2 w-full rounded-md border border-black/20 px-3 py-2.5 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setZcashRecipient(event.target.value)} placeholder="Testnet shielded address" spellCheck="false" value={zcashRecipient} /></div>
              <div><label className="text-sm font-medium" htmlFor="viewing-key">Zcash Testnet full viewing key</label><div className="mt-2 flex rounded-md border border-black/20 focus-within:border-black"><input className="min-w-0 flex-1 rounded-md px-3 py-2.5 text-sm outline-none" id="viewing-key" autoComplete="off" onChange={(event) => setViewingKey(event.target.value)} spellCheck="false" type={showKey ? "text" : "password"} value={viewingKey} placeholder="View-only key" /><button className="px-3 text-sm text-black/60 hover:text-black" onClick={() => setShowKey((visible) => !visible)} type="button">{showKey ? "Hide" : "Show"}</button></div><p className="mt-1 text-xs text-black/55">Required to verify payment. Do not enter a seed phrase or spending key.</p></div>
              {signedRequest && <>
                <div><label className="text-sm font-medium" htmlFor="zcash-txid">Zcash transaction ID</label><input id="zcash-txid" className="mt-2 w-full rounded-md border border-black/20 px-3 py-2.5 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setZcashTxid(event.target.value)} placeholder="Mined transaction ID" spellCheck="false" value={zcashTxid} /></div>
                <div><label className="text-sm font-medium" htmlFor="arb-tx">Arbitrum transaction hash</label><input id="arb-tx" className="mt-2 w-full rounded-md border border-black/20 px-3 py-2.5 text-sm outline-none focus:border-black" autoComplete="off" onChange={(event) => setArbitrumTxHash(event.target.value)} placeholder="fulfillSwap transaction hash" spellCheck="false" value={arbitrumTxHash} /></div>
                {intentId && <p className="break-all rounded-md bg-black/[0.04] p-3 text-xs"><span className="font-medium">Intent hash: </span>{intentId}</p>}
              </>}
              {error && <p className="rounded-md border border-black/20 p-3 text-sm" role="alert">{error}</p>}
              {notice && <p className="rounded-md bg-black/[0.04] p-3 text-sm" role="status">{notice}</p>}
              <button className="w-full rounded-md bg-black px-4 py-3 text-sm font-medium text-white hover:bg-black/80 disabled:cursor-not-allowed disabled:opacity-40" disabled={busy || (signedRequest ? !zcashTxid || !arbitrumTxHash : !wallet || !onTargetNetwork || !recipient || !zcashRecipient || !viewingKey)} type="submit">{busy ? "Submitting…" : signedRequest ? "Verify payments" : "Sign intent"}</button>
              <p className="text-xs text-black/55">{wallet ? `${compact(wallet)} · ` : "Wallet not connected · "}{onTargetNetwork ? "Arbitrum Sepolia connected" : "Select Arbitrum Sepolia"}</p>
            </form>
          </section>

          <section className="border-t border-black/10 pt-6 md:border-l md:border-t-0 md:pl-8 md:pt-0">
            <div className="flex items-center justify-between gap-3"><h2 className="text-lg font-semibold">Transaction status</h2><span className="text-xs text-black/55">{intentId ? "Updating" : "Waiting"}</span></div>
            <p className="mt-2 break-all text-xs text-black/55">{intentId || "Intent hash appears after signing"}</p>
            <ol className="mt-7 space-y-5">
              {STAGES.map((stage, index) => {
                const done = progress >= index;
                const active = progress + 1 === index && progress < STAGES.length - 1;
                return <li className="flex items-start gap-3" key={stage.title}><span className={`grid h-6 w-6 shrink-0 place-items-center rounded-full border text-xs ${done ? "border-black bg-black text-white" : "border-black/25 text-black/45"}`}>{done ? "✓" : index + 1}</span><div><p className={`text-sm font-medium ${done || active ? "text-black" : "text-black/45"}`}>{stage.title}</p><p className="mt-1 text-xs text-black/55">{done ? "Confirmed" : stage.detail}</p></div></li>;
              })}
            </ol>
            <p className="mt-7 border-t border-black/10 pt-4 text-sm text-black/70" role="status">{timelineMessage}</p>
            {progress >= 2 && arbitrumTxHash && <a className="mt-4 inline-block text-sm underline" href={`https://sepolia.arbiscan.io/tx/${arbitrumTxHash}`} target="_blank" rel="noreferrer">View Arbitrum transaction</a>}
          </section>
        </div>
      </div>
    </main>
  );
}


