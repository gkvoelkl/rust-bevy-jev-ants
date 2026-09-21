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

/// Overridable, so a proxy can be slipped in without touching the code.
pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
pub const ENDPOINT_PATH: &str = "/v1/systemone";
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// A successful round trip.
pub struct Reply {
    pub response: SystemOneResponse,
    /// Measured around the whole request, which is what the HUD shows.
    pub latency_ms: u32,
}

pub struct Client {
    base_url: String,
    key: String,
}

impl Client {
    /// Reads the key from `.env` or the environment. `None` means: play without
    /// a model rather than asking the player for a key they may not have.
    pub fn from_env() -> Option<Self> {
        #[cfg(not(target_arch = "wasm32"))]
        let _ = dotenvy::dotenv();

        let key = std::env::var("TYPESAFE_API_KEY")
            .ok()
            .map(|key| key.trim().to_string())
            .filter(|key| !key.is_empty())?;

        let base_url =
            std::env::var("TYPESAFE_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        Some(Self { base_url, key })
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
    })
}
