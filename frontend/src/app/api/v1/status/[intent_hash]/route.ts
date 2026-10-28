import { NextResponse } from "next/server";

const RELAYER_URL = (process.env.RELAYER_API_URL || "http://127.0.0.1:8080").replace(/\/+$/, "");

export async function GET(_request: Request, context: { params: Promise<{ intent_hash: string }> }) {
  const { intent_hash } = await context.params;
  try {
    const response = await fetch(`${RELAYER_URL}/api/v1/status/${encodeURIComponent(intent_hash)}`, { cache: "no-store" });
    const text = await response.text();
    let body: unknown;
    try { body = text ? JSON.parse(text) : {}; } catch { body = { error: text }; }
    return NextResponse.json(body, { status: response.status });
  } catch {
    return NextResponse.json({ error: "Could not reach the Rust relayer." }, { status: 502 });
  }
}
