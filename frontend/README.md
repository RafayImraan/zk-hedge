# ZK-Hedge dashboard

Next.js App Router dashboard using Tailwind CSS 4. The wallet flow signs the
intent with `personal_sign`; the `/api/intent` route proxies it to the local
relayer configured by `RELAYER_API_URL` (defaults to `http://127.0.0.1:8080`).
Set an override in the Next.js process environment or `frontend/.env.local`.

The relayer should implement `POST /api/intent` and return an `intent_id` plus
a status. The dashboard polls `GET /api/intent?intent_id=...` for later stages.
The current Rust relayer crate has the proof verifier but does not yet run this
HTTP API, so submissions show a connection error until that server is available.

The browser creates a one-time 32-byte demo value for `ephemeral_viewing_key`
and discards it from memory after signing. It is not yet a Zcash Sapling key.

```sh
npm install
npm run dev
```

The receive side is a fixed indicative quote of 25.00 USDC and is not included
in the current relayer intent schema.
