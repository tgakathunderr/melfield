//! Hugging Face sign-in for gated models (Indic Parler, Pocket cloning).
//! OAuth (PKCE, loopback redirect) when a Melfield client id is configured
//! (once registered); otherwise a paste-a-read-token fallback. The token lives in
//! the OS keychain, never in the database or logs.

use std::sync::Arc;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{CoreEvent, Melfield};
use crate::error::{ErrorCode, MelError, MelResult};

/// Public OAuth client id for the Melfield Hugging Face app. Empty until registered.
pub const HF_CLIENT_ID: &str = "";
const KEYRING_SERVICE: &str = "app.melfield";
const KEYRING_USER: &str = "huggingface";

fn entry() -> Option<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).ok()
}

pub fn load_token() -> Option<String> {
    if let Ok(t) = std::env::var("MELFIELD_HF_TOKEN") {
        return Some(t);
    }
    entry()
        .and_then(|e| e.get_password().ok())
        .filter(|t| !t.is_empty())
}

fn save_token(token: &str) -> MelResult<()> {
    entry()
        .ok_or_else(|| MelError::internal("keychain unavailable"))?
        .set_password(token)
        .map_err(|e| {
            MelError::new(
                ErrorCode::Internal,
                "couldn't save your sign-in safely on this computer",
            )
            .with_detail(e)
        })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HfAccount {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuthStart {
    pub authorize_url: String,
}

#[derive(Deserialize)]
struct WhoAmI {
    name: String,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

impl Melfield {
    pub fn hf_oauth_available(&self) -> bool {
        !HF_CLIENT_ID.is_empty()
    }

    async fn whoami(&self, token: &str) -> MelResult<HfAccount> {
        let resp = self
            .http
            .get("https://huggingface.co/api/whoami-v2")
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| MelError::offline().with_detail(e))?;
        if !resp.status().is_success() {
            return Err(MelError::new(
                ErrorCode::NeedsSignIn,
                "that key didn't work — copy it again from hugging face",
            ));
        }
        let who: WhoAmI = resp.json().await.map_err(MelError::internal)?;
        Ok(HfAccount { name: who.name })
    }

    /// Fallback flow: the person pastes a read token.
    pub async fn hf_sign_in_with_token(&self, token: &str) -> MelResult<HfAccount> {
        let token = token.trim();
        if !token.starts_with("hf_") {
            return Err(MelError::new(
                ErrorCode::NeedsSignIn,
                "that doesn't look like a hugging face key — it starts with hf_",
            ));
        }
        let account = self.whoami(token).await?;
        save_token(token)?;
        self.supervisor.set_hf_token(Some(token.to_string()));
        self.emit(CoreEvent::SettingsChanged);
        self.emit(CoreEvent::VoicesChanged);
        Ok(account)
    }

    pub fn hf_sign_out(&self) -> MelResult<()> {
        if let Some(e) = entry() {
            let _ = e.delete_credential();
        }
        self.supervisor.set_hf_token(None);
        self.emit(CoreEvent::SettingsChanged);
        Ok(())
    }

    /// OAuth: start a loopback listener and return the URL to open in the browser.
    pub async fn hf_oauth_begin(self: &Arc<Self>) -> MelResult<OAuthStart> {
        if HF_CLIENT_ID.is_empty() {
            return Err(MelError::new(
                ErrorCode::Unsupported,
                "sign-in with a key instead",
            ));
        }
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let verifier: String = (0..64)
            .map(|_| char::from(b'a' + rand::random::<u8>() % 26))
            .collect();
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        let state: String = (0..24)
            .map(|_| char::from(b'a' + rand::random::<u8>() % 26))
            .collect();
        let mut url = url::Url::parse("https://huggingface.co/oauth/authorize")
            .map_err(MelError::internal)?;
        url.query_pairs_mut()
            .append_pair("client_id", HF_CLIENT_ID)
            .append_pair("redirect_uri", &redirect)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid profile gated-repos")
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        let me = self.clone();
        self.rt.spawn(async move {
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(600),
                accept_code(listener, &state),
            )
            .await;
            let Ok(Ok(code)) = result else {
                tracing::warn!("hf oauth timed out or failed");
                return;
            };
            match me.exchange_code(&code, &verifier, &redirect).await {
                Ok(token) => {
                    if save_token(&token).is_ok() {
                        me.supervisor.set_hf_token(Some(token));
                        me.emit(CoreEvent::SettingsChanged);
                        me.emit(CoreEvent::VoicesChanged);
                    }
                }
                Err(e) => tracing::warn!(detail = %e.detail, "hf token exchange failed"),
            }
        });
        Ok(OAuthStart {
            authorize_url: url.to_string(),
        })
    }

    async fn exchange_code(&self, code: &str, verifier: &str, redirect: &str) -> MelResult<String> {
        let resp = self
            .http
            .post("https://huggingface.co/oauth/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", redirect),
                ("client_id", HF_CLIENT_ID),
                ("code_verifier", verifier),
            ])
            .send()
            .await
            .map_err(|e| MelError::offline().with_detail(e))?;
        if !resp.status().is_success() {
            return Err(MelError::internal(format!(
                "token endpoint {}",
                resp.status()
            )));
        }
        Ok(resp
            .json::<TokenResponse>()
            .await
            .map_err(MelError::internal)?
            .access_token)
    }

    /// Has the signed-in person been granted access to this recipe's gated repo?
    pub async fn hf_check_access(&self, recipe_id: &str) -> MelResult<bool> {
        let recipe = self
            .registry
            .read()
            .recipe(recipe_id)
            .cloned()
            .ok_or_else(|| MelError::internal("unknown recipe"))?;
        let Some(artifact) = recipe.artifacts.iter().find(|a| a.gated) else {
            return Ok(true);
        };
        let Some(file) = artifact.files.first() else {
            return Ok(true);
        };
        let Some(token) = load_token() else {
            return Ok(false);
        };
        let resp = self
            .http
            .head(artifact.url(file))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| MelError::offline().with_detail(e))?;
        Ok(resp.status().is_success() || resp.status().is_redirection())
    }
}

/// Minimal loopback HTTP handler: read the callback, check state, reply with a calm page.
async fn accept_code(listener: tokio::net::TcpListener, expected_state: &str) -> MelResult<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await?;
        let req = String::from_utf8_lossy(&buf[..n]);
        let path = req
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");
        let parsed = url::Url::parse(&format!("http://localhost{path}")).ok();
        let get = |k: &str| {
            parsed.as_ref().and_then(|u| {
                u.query_pairs()
                    .find(|(kk, _)| kk == k)
                    .map(|(_, v)| v.to_string())
            })
        };
        let (code, state) = (get("code"), get("state"));
        let ok = code.is_some() && state.as_deref() == Some(expected_state);
        let body = if ok {
            "<html><body style=\"background:#050506;color:#EDEDEF;font-family:sans-serif;display:grid;place-items:center;height:100vh\"><p>you're signed in — head back to melfield.</p></body></html>"
        } else {
            "<html><body style=\"background:#050506;color:#EDEDEF;font-family:sans-serif;display:grid;place-items:center;height:100vh\"><p>that didn't work — try signing in again from melfield.</p></body></html>"
        };
        let resp = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        let _ = sock.write_all(resp.as_bytes()).await;
        if ok {
            return Ok(code.unwrap_or_default());
        }
    }
}
