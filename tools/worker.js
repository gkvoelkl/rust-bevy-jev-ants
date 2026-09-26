// The proxy the page on GitHub Pages talks to. A Cloudflare Worker, because
// Pages serves files and nothing else — there is no `/api` on its origin, and
// no place to put a forwarding rule.
//
// It does the same one thing `tools/dev-proxy.py` does, plus the part a proxy
// on another origin cannot avoid: answering the CORS preflight itself.
//
//   * `Origin` is dropped. The API reads it on the request itself and refuses
//     one it does not know with `400 Disallowed CORS origin` (measured
//     2026-09-24). A browser attaches it to every POST, and `fetch` inside a
//     Worker attaches nothing that is not asked for — so simply not copying it
//     across is the whole trick.
//   * The preflight is answered here. The API refuses those from every origin,
//     its own console included, so nothing upstream could answer it.
//
// It handles no keys. The player's `Authorization` header is passed through as
// it arrives and is never written down, and this file has no key of its own to
// leak — a key never typed into this game does not become useful by arriving
// here.
//
// Deploy:
//
//     cd tools && npx wrangler deploy
//
// and put the URL it prints into the repository variable `ANTS_API_BASE`
// (`gh variable set ANTS_API_BASE --body https://…workers.dev`), which the
// Pages workflow bakes into the bundle.

const UPSTREAM = "https://api.typesafe.ai";

// The one path the game calls. Anything else is refused, so this cannot be
// picked up and used as a general relay to the API — it is a door to one room.
const ALLOWED_PATH = "/v1/systemone";

// Who may call it. Not a security measure — anyone with a terminal can claim
// any origin — but it keeps another page from quietly spending this Worker's
// free quota, and a browser cannot lie here. Add an origin when the game is
// served from somewhere new.
const ALLOWED_ORIGINS = [
  "https://gkvoelkl.github.io",
  "http://localhost:8080",
  "http://127.0.0.1:8080",
];

function corsHeaders(origin) {
  return {
    // Echoed rather than `*`, because a request carrying an `Authorization`
    // header is a credentialed one in spirit and `*` reads as careless.
    "Access-Control-Allow-Origin": origin,
    "Access-Control-Allow-Methods": "POST, OPTIONS",
    "Access-Control-Allow-Headers": "authorization, content-type",
    // A day. The preflight is the same answer every time, and an ant asking
    // twice a second should not pay for it twice.
    "Access-Control-Max-Age": "86400",
    Vary: "Origin",
  };
}

export default {
  async fetch(request) {
    const origin = request.headers.get("Origin") ?? "";
    const allowed = ALLOWED_ORIGINS.includes(origin);
    const path = new URL(request.url).pathname;

    if (request.method === "OPTIONS") {
      // Without the headers, a browser reads this as "no" and never sends the
      // real request — which is exactly what the API does to a page.
      return allowed
        ? new Response(null, { status: 204, headers: corsHeaders(origin) })
        : new Response("unknown origin", { status: 403 });
    }

    if (request.method !== "POST" || path !== ALLOWED_PATH) {
      return new Response(`only POST ${ALLOWED_PATH}`, { status: 404 });
    }

    if (!allowed) {
      return new Response("unknown origin", { status: 403 });
    }

    // Built by hand rather than copied, so nothing travels that was not meant
    // to: `Origin` and `Referer` are left behind here, and the key is the only
    // thing of the player's that goes on.
    const forwarded = new Request(UPSTREAM + path, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: request.headers.get("Authorization") ?? "",
      },
      body: await request.text(),
    });

    // Passed back whole, refusals included. The game shows what the API said in
    // its inspector, and a 401 swallowed here would look like a dead line.
    const answer = await fetch(forwarded);
    const headers = new Headers(corsHeaders(origin));
    headers.set(
      "Content-Type",
      answer.headers.get("Content-Type") ?? "application/json",
    );

    return new Response(answer.body, { status: answer.status, headers });
  },
};
