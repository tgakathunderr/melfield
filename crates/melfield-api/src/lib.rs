//! Local API: OpenAI-compatible `POST /v1/audio/speech` on 127.0.0.1.
//! Off by default (Settings › Advanced). Optional bearer token. See docs/api.md.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use melfield_core::audio::codec;
use melfield_core::error::{ErrorCode, MelError};
use melfield_core::pull::CancelToken;
use melfield_core::service::SpeakRequest;
use melfield_core::Melfield;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::{oneshot, Semaphore};

const MAX_BODY: usize = 4 * 1024 * 1024;
/// Concurrent syntheses; extra requests wait, beyond QUEUE they get 429.
const CONCURRENCY: usize = 2;
const QUEUE: usize = 16;

#[derive(Clone)]
struct AppState {
    mf: Arc<Melfield>,
    token: Option<String>,
    permits: Arc<Semaphore>,
    waiting: Arc<std::sync::atomic::AtomicUsize>,
}

pub struct ApiServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
}

impl ApiServer {
    pub fn stop(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

impl Drop for ApiServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}

pub fn router(mf: Arc<Melfield>, token: Option<String>) -> Router {
    let state = AppState {
        mf,
        token,
        permits: Arc::new(Semaphore::new(CONCURRENCY)),
        waiting: Default::default(),
    };
    Router::new()
        .route("/health", get(health))
        .route("/v1/models", get(models))
        .route("/v1/voices", get(voices))
        .route("/v1/audio/speech", post(speech))
        .layer(DefaultBodyLimit::max(MAX_BODY))
        .with_state(state)
}

/// Bind to loopback only and serve until stopped.
pub async fn start(
    mf: Arc<Melfield>,
    port: u16,
    token: Option<String>,
) -> Result<ApiServer, MelError> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        MelError::new(
            ErrorCode::Busy,
            format!("port {port} is already in use — pick another in settings"),
        )
        .with_detail(e)
    })?;
    let addr = listener.local_addr().map_err(MelError::internal)?;
    let (tx, rx) = oneshot::channel::<()>();
    let app = router(mf, token);
    tokio::spawn(async move {
        let serve = axum::serve(listener, app).with_graceful_shutdown(async {
            let _ = rx.await;
        });
        if let Err(e) = serve.await {
            tracing::warn!(error = %e, "local api stopped");
        }
    });
    tracing::info!(%addr, "local api listening");
    Ok(ApiServer {
        addr,
        shutdown: Some(tx),
    })
}

#[derive(Serialize)]
struct ApiErrorBody {
    error: ApiErrorInner,
}

#[derive(Serialize)]
struct ApiErrorInner {
    message: String,
    r#type: String,
    code: String,
}

fn api_error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    let body = ApiErrorBody {
        error: ApiErrorInner {
            message: message.into(),
            r#type: "invalid_request_error".into(),
            code: code.into(),
        },
    };
    (status, Json(body)).into_response()
}

fn from_mel(e: MelError) -> Response {
    let status = match e.code {
        ErrorCode::VoiceNotFound | ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::EmptyText | ErrorCode::TooLong | ErrorCode::Unsupported => {
            StatusCode::BAD_REQUEST
        }
        ErrorCode::VoiceNotInstalled | ErrorCode::EngineStarting => StatusCode::CONFLICT,
        ErrorCode::Busy => StatusCode::TOO_MANY_REQUESTS,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let code = serde_json::to_value(e.code)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_default();
    api_error(status, &code, e.message)
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(expected) = &state.token else {
        return true;
    };
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|t| constant_time_eq(t.as_bytes(), expected.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn health() -> impl IntoResponse {
    Json(json!({"ok": true, "version": env!("CARGO_PKG_VERSION")}))
}

async fn models(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&s, &headers) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or wrong api key",
        );
    }
    let voices = match s.mf.voices() {
        Ok(v) => v,
        Err(e) => return from_mel(e),
    };
    let mut ids: Vec<String> = voices
        .iter()
        .filter(|v| v.installed)
        .map(|v| v.recipe_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    let data: Vec<_> = ids
        .into_iter()
        .map(|id| json!({"id": id, "object": "model", "owned_by": "melfield"}))
        .collect();
    Json(json!({"object": "list", "data": data})).into_response()
}

async fn voices(State(s): State<AppState>, headers: HeaderMap) -> Response {
    if !authorized(&s, &headers) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or wrong api key",
        );
    }
    match s.mf.voices() {
        Ok(v) => {
            let data: Vec<_> = v
                .iter()
                .map(|v| json!({"id": v.id, "name": v.name, "languages": v.languages, "installed": v.installed, "kind": v.kind}))
                .collect();
            Json(json!({"object": "list", "data": data})).into_response()
        }
        Err(e) => from_mel(e),
    }
}

#[derive(Deserialize)]
struct SpeechBody {
    #[allow(dead_code)]
    model: Option<String>,
    input: String,
    voice: Option<String>,
    response_format: Option<String>,
    speed: Option<f32>,
    language: Option<String>,
}

/// Resolve an OpenAI-style voice name ("alloy") or a Melfield id/name to a voice id.
fn resolve_voice(mf: &Melfield, requested: Option<&str>) -> Result<String, MelError> {
    let voices = mf.voices()?;
    let default = mf.settings()?.default_voice;
    let wanted = requested.unwrap_or(&default).to_lowercase();
    if let Some(v) = voices
        .iter()
        .find(|v| v.id == wanted || v.name.to_lowercase() == wanted)
    {
        return Ok(v.id.clone());
    }
    // Unknown OpenAI voice names map to the default voice.
    const OPENAI: &[&str] = &[
        "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer",
        "verse",
    ];
    if OPENAI.contains(&wanted.as_str()) {
        return Ok(default);
    }
    Err(MelError::new(
        ErrorCode::VoiceNotFound,
        format!("no voice called {wanted} — GET /v1/voices lists them"),
    ))
}

async fn speech(
    State(s): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<SpeechBody>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if !authorized(&s, &headers) {
        return api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or wrong api key",
        );
    }
    let Json(body) = match body {
        Ok(b) => b,
        Err(e) => {
            return api_error(
                StatusCode::BAD_REQUEST,
                "invalid_json",
                format!("couldn't read the request: {}", e.body_text()),
            )
        }
    };
    let format = body
        .response_format
        .as_deref()
        .unwrap_or("wav")
        .to_lowercase();
    if !matches!(format.as_str(), "wav" | "flac" | "pcm") {
        return api_error(
            StatusCode::BAD_REQUEST,
            "unsupported_format",
            format!("{format} isn't available yet — use wav, flac or pcm"),
        );
    }
    let voice_id = match resolve_voice(&s.mf, body.voice.as_deref()) {
        Ok(v) => v,
        Err(e) => return from_mel(e),
    };
    let speed = body.speed.unwrap_or(1.0);
    if !(0.25..=4.0).contains(&speed) {
        return api_error(
            StatusCode::BAD_REQUEST,
            "invalid_speed",
            "speed must be between 0.25 and 4.0",
        );
    }
    // Simple backpressure: bounded queue in front of a small semaphore.
    use std::sync::atomic::Ordering;
    if s.waiting.fetch_add(1, Ordering::SeqCst) >= QUEUE {
        s.waiting.fetch_sub(1, Ordering::SeqCst);
        return api_error(
            StatusCode::TOO_MANY_REQUESTS,
            "busy",
            "melfield is busy — try again in a moment",
        );
    }
    let permit = s.permits.clone().acquire_owned().await;
    s.waiting.fetch_sub(1, Ordering::SeqCst);
    let Ok(_permit) = permit else {
        return api_error(StatusCode::SERVICE_UNAVAILABLE, "closing", "shutting down");
    };

    let req = SpeakRequest {
        text: body.input,
        voice_id,
        speed: speed.clamp(0.5, 2.0),
        delivery: Default::default(),
        language: body.language,
        play: false,
    };
    let mf = s.mf.clone();
    let result =
        tokio::task::spawn_blocking(move || mf.synthesize_blocking(&req, &CancelToken::default()))
            .await;
    let (pcm, _) = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => return from_mel(e),
        Err(e) => return from_mel(MelError::internal(e)),
    };
    let (bytes, mime) = match format.as_str() {
        "flac" => (codec::flac_bytes(&pcm), "audio/flac"),
        "pcm" => (
            Ok(pcm
                .samples
                .iter()
                .flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes())
                .collect()),
            "audio/pcm",
        ),
        _ => (codec::wav_bytes(&pcm), "audio/wav"),
    };
    match bytes {
        Ok(b) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .header("x-sample-rate", pcm.sample_rate.to_string())
            .body(Body::from(b))
            .unwrap_or_else(|_| {
                api_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "couldn't encode audio",
                )
            }),
        Err(e) => from_mel(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use melfield_core::{Config, NullSink};

    async fn melfield() -> (tempfile::TempDir, Arc<Melfield>) {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config {
            root: Some(dir.path().to_path_buf()),
            resources_dir: dir.path().join("r"),
            uv_path: "uv".into(),
            app_version: "t".into(),
            headless: true,
        };
        let mf = Melfield::new(cfg, Arc::new(NullSink)).unwrap();
        (dir, mf)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn health_and_auth() {
        let (_d, mf) = melfield().await;
        let server = start(mf, 0, Some("secret".into())).await.unwrap();
        assert!(server.addr.ip().is_loopback());
        let base = format!("http://{}", server.addr);
        let c = reqwest::Client::new();
        assert_eq!(
            c.get(format!("{base}/health"))
                .send()
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            c.get(format!("{base}/v1/voices"))
                .send()
                .await
                .unwrap()
                .status(),
            401
        );
        let ok = c
            .get(format!("{base}/v1/voices"))
            .bearer_auth("secret")
            .send()
            .await
            .unwrap();
        assert_eq!(ok.status(), 200);
        let v: serde_json::Value = ok.json().await.unwrap();
        assert!(v["data"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["id"] == "aria"));
        server.stop();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn speech_validates_input_with_openai_errors() {
        let (_d, mf) = melfield().await;
        let server = start(mf, 0, None).await.unwrap();
        let url = format!("http://{}/v1/audio/speech", server.addr);
        let c = reqwest::Client::new();
        let r = c.post(&url).json(&json!({"model": "tts-1", "input": "hi", "voice": "alloy", "response_format": "mp3"})).send().await.unwrap();
        assert_eq!(r.status(), 400);
        let body: serde_json::Value = r.json().await.unwrap();
        assert_eq!(body["error"]["code"], "unsupported_format");
        let r = c
            .post(&url)
            .json(&json!({"input": "hi", "voice": "nobody"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 404);
        // Voice exists but isn't installed in a fresh data dir.
        let r = c
            .post(&url)
            .json(&json!({"input": "hi", "voice": "aria"}))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 409);
        let r = c
            .post(&url)
            .header("content-type", "application/json")
            .body("{not json")
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), 400);
    }

    #[test]
    fn constant_time_compare() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"ab"));
    }
}
