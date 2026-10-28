import { NextResponse } from "next/server";

const RELAYER_URL = (process.env.RELAYER_API_URL || "http://127.0.0.1:8080").replace(/\/+$/, "");

export async function POST(request: Request) {
  let payload: unknown;
  try { payload = await request.json(); }
  catch { return NextResponse.json({ error: "Request body must be valid JSON." }, { status: 400 }); }
  try {
    const response = await fetch(`${RELAYER_URL}/api/v1/intent`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(payload),
      cache: "no-store",
    });
    const text = await response.text();
    let body: unknown;
    try { body = text ? JSON.parse(text) : {}; } catch { body = { error: text }; }
    return NextResponse.json(body, { status: response.status });
  } catch {
    return NextResponse.json({ error: "Could not reach the Rust relayer at 127.0.0.1:8080." }, { status: 502 });
  }
}
