//! A minimal TypeSafe client. Knows nothing about ants.
//!
//! One POST with JSON, three answer types. `ehttp` carries it, which needs no
//! async runtime next to Bevy's scheduler and maps to `fetch` in a browser.
//! Nothing here ever blocks the caller: the answer arrives over a channel.

pub mod types;

use std::time::Duration;

use bevy::platform::time::Instant;
use crossbeam_channel::{Receiver, bounded};

use types::{ApiError, SystemOneRequest, SystemOneResponse};

/// Overridable through `TYPESAFE_BASE_URL`, so a proxy can be slipped in
/// without touching the code.
#[cfg(not(target_arch = "wasm32"))]
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";

/// The browser cannot reach the API directly, so the web build asks whoever
/// served the page and lets that forward the request.
///
/// This is `ANTS.md` §5.3 answered, and the answer is: blocked. Measured on
/// 2026-09-24, in two parts, because the API refuses a page twice over:
///
///   * The CORS **preflight** for `/v1/systemone` came back `400 Disallowed
///     CORS origin` from every origin tried — `http://localhost:8080`, `:3000`,
///     `:5173`, `http://127.0.0.1:8080`, `null`, and the API's own
///     `https://console.typesafe.ai` — with no `access-control-allow-origin`
///     header at all. An `Authorization` header always forces a preflight, so a
///     page never gets to send the request. Direct calls are out.
///   * The **request itself** is judged by its `Origin` too, and there the
///     allowlist is real: the same POST that a browser refuses is answered
///     `422` (body invalid, auth fine) with no `Origin`, or with the console's
///     — and `400 Disallowed CORS origin` with `http://localhost:8080`.
///
/// The second half is the one that bites a proxy, because a browser attaches
/// `Origin` to every POST, same-origin or not, and a forwarding proxy hands it
/// straight on. Whatever forwards `/api` must therefore **drop that header**:
/// `tools/dev-proxy.py` while developing, `proxy_set_header Origin "";` or its
/// equivalent in a deployment.
///
/// A relative URL rather than a configured host, because where the page is
/// served by something that can forward, the proxy *is* the configuration. The
/// player's key rides in the `Authorization` header and is only passed on —
/// never part of the URL, so it stays out of logs and out of the address bar.
///
/// `ANTS_API_BASE` at build time points the bundle somewhere else, which is
/// what a static host needs: GitHub Pages serves files and nothing more, so
/// there is no `/api` on its origin to forward anything. The Pages workflow
/// sets this to the standalone proxy (`tools/worker.js`), and because that
/// proxy then lives on another origin, it has to answer the preflight itself —
/// which it can, being ours. `trunk serve` sets nothing and keeps using
/// `tools/dev-proxy.py` next door.
///
/// Read at compile time, and cargo does not watch environment variables: after
/// changing it by hand, `touch src/api/mod.rs`. In CI the build is fresh
/// anyway.
#[cfg(target_arch = "wasm32")]
pub const DEFAULT_BASE_URL: &str = match option_env!("ANTS_API_BASE") {
    Some(url) => url,
    None => "/api",
};

pub const ENDPOINT_PATH: &str = "/v1/systemone";
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A successful round trip.
pub struct Reply {
    pub response: SystemOneResponse,
    /// Measured around the whole request, which is what the HUD shows.
    pub latency_ms: u32,
    /// The body exactly as it arrived, before anything here made sense of it.
    ///
    /// Kept so the inspector can show what the API really said rather than our
    /// reading of it. That difference is the point: an answer type this game
    /// does not model yet parses as `Unsupported` and would vanish from a
    /// re-serialised copy, while here it is plainly there to be seen.
    pub body: String,
}

/// `.env` is read once and its values land in the environment, so calling this
/// again is cheap. There is no `.env` in a browser, and no file system to look
/// for one in.
fn load_dotenv() {
    #[cfg(not(target_arch = "wasm32"))]
    let _ = dotenvy::dotenv();
}

pub struct Client {
    base_url: String,
    key: String,
}

impl Client {
    /// Reads the key from `.env` or the environment. `None` means there is none
    /// there — the game then asks the player for one (`decisions::KeyPrompt`).
    pub fn from_env() -> Option<Self> {
        load_dotenv();

        let key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())?;

        Some(Self::with_key(key))
    }

    /// A key the player typed into the dialog. It lives in this struct and
    /// nowhere else: nothing here writes it to a file, an environment variable
    /// or a save game, so it is gone when the window closes. Typing it again
    /// next time is the price of not storing a secret behind the player's back
    /// — `.env` is there for anyone who would rather not.
    pub fn with_key(key: String) -> Self {
        load_dotenv();

        let base_url =
            std::env::var("TYPESAFE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        Self { base_url, key }
    }

    /// Fires the request and returns at once. The key never leaves this struct.
    pub fn send(&self, request: &SystemOneRequest) -> Pending {
        let (sender, receiver) = bounded(1);
        let started = Instant::now();

        let url = format!("{}{ENDPOINT_PATH}", self.base_url);
        let http = match ehttp::Request::post_json(url, request) {
            Ok(http) => http
                .with_header("Authorization", format!("Bearer {}", self.key))
                .with_timeout(Some(REQUEST_TIMEOUT)),
            Err(error) => {
                let _ = sender.send(Err(ApiError::Decode(error.to_string())));
                return Pending { receiver };
            }
        };

        ehttp::fetch(http, move |result| {
            let _ = sender.send(interpret(result, started));
        });

        Pending { receiver }
    }
}

/// A request in flight. Polling it never blocks.
pub struct Pending {
    receiver: Receiver<Result<Reply, ApiError>>,
}

impl Pending {
    pub fn poll(&self) -> Option<Result<Reply, ApiError>> {
        self.receiver.try_recv().ok()
    }
}

fn interpret(result: ehttp::Result<ehttp::Response>, started: Instant) -> Result<Reply, ApiError> {
    let latency_ms = started.elapsed().as_millis() as u32;

    let response = result.map_err(|error| ApiError::Transport(error.to_string()))?;
    if !response.ok {
        let body = response.text().unwrap_or_default().to_string();
        return Err(ApiError::from_status(response.status, body));
    }

    let parsed: SystemOneResponse = serde_json::from_slice(&response.bytes)
        .map_err(|error| ApiError::Decode(error.to_string()))?;

    Ok(Reply {
        response: parsed,
        latency_ms,
        body: String::from_utf8_lossy(&response.bytes).into_owned(),
    })
}
