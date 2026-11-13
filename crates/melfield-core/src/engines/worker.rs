//! One Python worker process speaking the stdio protocol.

use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use serde_json::{json, Value};

use super::protocol::{self, Message};
use crate::error::{ErrorCode, FixAction, MelError, MelResult};

pub struct WorkerSpec {
    pub family: String,
    pub python: PathBuf,
    pub workers_dir: PathBuf,
    pub hf_cache: PathBuf,
    pub log_file: PathBuf,
    pub hf_token: Option<String>,
}

pub struct WorkerProcess {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    pub family: String,
    pub loaded: Option<String>,
}

impl WorkerProcess {
    pub fn spawn(spec: &WorkerSpec) -> MelResult<Self> {
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.log_file)?;
        let mut cmd = Command::new(&spec.python);
        cmd.args(["-u", "-m", "melfield_worker", "--family", &spec.family])
            .env("PYTHONPATH", &spec.workers_dir)
            .env("PYTHONIOENCODING", "utf-8")
            .env("PYTHONNOUSERSITE", "1")
            .env("HF_HUB_CACHE", &spec.hf_cache)
            .env("HF_HUB_OFFLINE", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("TOKENIZERS_PARALLELISM", "false")
            .env("PYTORCH_ENABLE_MPS_FALLBACK", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::from(log));
        if let Some(t) = &spec.hf_token {
            cmd.env("HF_TOKEN", t);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| MelError::engine_broken(&spec.family).with_detail(e))?;
        let stdin = BufWriter::new(
            child
                .stdin
                .take()
                .ok_or_else(|| MelError::internal("no worker stdin"))?,
        );
        let stdout = BufReader::new(
            child
                .stdout
                .take()
                .ok_or_else(|| MelError::internal("no worker stdout"))?,
        );
        let mut w = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            family: spec.family.clone(),
            loaded: None,
        };
        let hello = w.call(
            json!({"op": "hello", "protocol": protocol::PROTOCOL_VERSION}),
            &mut |_| true,
        )?;
        tracing::info!(family = %spec.family, hello = %hello.header, "worker ready");
        Ok(w)
    }

    /// Send a request and collect events until the final response.
    /// `on_event` returns false to request cancellation.
    pub fn call(
        &mut self,
        mut req: Value,
        on_event: &mut dyn FnMut(&Message) -> bool,
    ) -> MelResult<Message> {
        let id = self.next_id;
        self.next_id += 1;
        req["id"] = json!(id);
        protocol::write_json(&mut self.stdin, &req).map_err(|e| self.crashed(e))?;
        let mut cancel_sent = false;
        loop {
            let msg = protocol::read_message(&mut self.stdout).map_err(|e| self.crashed(e))?;
            if msg.header.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if msg.header.get("event").is_some() {
                if !on_event(&msg) && !cancel_sent {
                    let cid = self.next_id;
                    self.next_id += 1;
                    protocol::write_json(
                        &mut self.stdin,
                        &json!({"id": cid, "op": "cancel", "target": id}),
                    )
                    .map_err(|e| self.crashed(e))?;
                    cancel_sent = true;
                }
                continue;
            }
            if msg.header.get("ok").and_then(Value::as_bool) == Some(true) {
                return Ok(msg);
            }
            return Err(map_worker_error(&self.family, &msg.header));
        }
    }

    fn crashed(&mut self, e: std::io::Error) -> MelError {
        let status = self.child.try_wait().ok().flatten();
        MelError::new(
            ErrorCode::EngineCrashed,
            "the voice engine stopped unexpectedly — trying again",
        )
        .with_fix(FixAction::Retry)
        .with_detail(format!(
            "{} worker io error: {e}; exit status {status:?}",
            self.family
        ))
    }

    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    pub fn shutdown(mut self) {
        let _ = protocol::write_json(&mut self.stdin, &json!({"id": 0, "op": "shutdown"}));
        for _ in 0..20 {
            if !self.is_alive() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = self.child.kill();
    }
}

impl Drop for WorkerProcess {
    fn drop(&mut self) {
        if self.is_alive() {
            let _ = self.child.kill();
        }
    }
}

/// Map worker error codes (strings) to human errors.
pub fn map_worker_error(family: &str, header: &Value) -> MelError {
    let code = header
        .pointer("/error/code")
        .and_then(Value::as_str)
        .unwrap_or("INTERNAL");
    let detail = header
        .pointer("/error/detail")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let e = match code {
        "OUT_OF_MEMORY" => MelError::out_of_memory(),
        "DEVICE_UNAVAILABLE" => MelError::new(
            ErrorCode::Unsupported,
            "your graphics card is busy or unavailable — we'll use the processor instead",
        )
        .with_fix(FixAction::Retry),
        "MODEL_FILES_MISSING" => MelError::engine_broken(family),
        "WATERMARK_UNAVAILABLE" => MelError::new(
            ErrorCode::WatermarkUnavailable,
            "cloned voices need their safety watermark — repair this voice to restore it",
        )
        .with_fix(FixAction::Repair {
            family: family.to_string(),
        }),
        "UNSUPPORTED_LANGUAGE" => MelError::new(
            ErrorCode::Unsupported,
            "this voice doesn't speak that language",
        ),
        "BAD_REFERENCE_AUDIO" => MelError::new(
            ErrorCode::BadAudio,
            "that recording didn't work for cloning — try a clearer 10–30 second clip",
        ),
        "CANCELLED" => MelError::cancelled(),
        "DEPENDENCY_MISSING" => MelError::engine_broken(family),
        _ => MelError::new(
            ErrorCode::EngineCrashed,
            "the voice engine hit a snag — try again",
        )
        .with_fix(FixAction::Retry),
    };
    let e = e.with_detail(format!("{family}: {code}: {detail}"));
    tracing::warn!(detail = %e.detail, "worker error");
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_codes_to_human_errors() {
        let e = map_worker_error(
            "qwen3",
            &json!({"ok": false, "error": {"code": "OUT_OF_MEMORY", "detail": "CUDA OOM"}}),
        );
        assert_eq!(e.code, ErrorCode::OutOfMemory);
        assert!(!e.message.contains("CUDA"));
        let e = map_worker_error(
            "chatterbox",
            &json!({"ok": false, "error": {"code": "WATERMARK_UNAVAILABLE"}}),
        );
        assert_eq!(e.code, ErrorCode::WatermarkUnavailable);
        assert_eq!(
            e.fix,
            Some(FixAction::Repair {
                family: "chatterbox".into()
            })
        );
        let e = map_worker_error("x", &json!({"ok": false}));
        assert_eq!(e.code, ErrorCode::EngineCrashed);
    }
}
