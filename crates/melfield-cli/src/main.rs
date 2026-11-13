//! `melfield` CLI — the Ollama-style companion to the app.
//!
//!   melfield list [--installed]
//!   melfield pull <voice>
//!   melfield say "text" [-v aria] [-o out.wav] [--play]
//!   melfield serve [--port 5150]
//!
//! Uses the same data folder as the app. Errors print the same calm messages.

use std::path::PathBuf;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use melfield_core::audio::codec::{self, Format};
use melfield_core::pull::CancelToken;
use melfield_core::service::SpeakRequest;
use melfield_core::{Config, CoreEvent, EventSink, MelError, Melfield};

#[derive(Parser)]
#[command(name = "melfield", version, about = "every voice. on your machine.")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List voices
    List {
        #[arg(long)]
        installed: bool,
    },
    /// Download a voice
    Pull { voice: String },
    /// Speak text with a voice
    Say {
        text: String,
        #[arg(short, long)]
        voice: Option<String>,
        /// Output file (.wav or .flac). Defaults to melfield-<time>.wav
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long, default_value_t = 1.0)]
        speed: f32,
        /// Play through the speakers too
        #[arg(long)]
        play: bool,
    },
    /// Run the local OpenAI-compatible API (127.0.0.1 only)
    Serve {
        #[arg(long, default_value_t = 5150)]
        port: u16,
        /// Require this bearer token
        #[arg(long)]
        token: Option<String>,
    },
}

struct ConsoleSink;
impl EventSink for ConsoleSink {
    fn emit(&self, event: CoreEvent) {
        match event {
            CoreEvent::Pull(p) => {
                let pct = p.bytes_done as f64 / p.bytes_total.max(1) as f64 * 100.0;
                eprint!("\r  ↓ {:>5.1}%  {:?}        ", pct, p.state);
            }
            CoreEvent::Env(e) => eprint!("\r  ○ {}        ", e.detail),
            _ => {}
        }
    }
}

/// Resources live next to the binary when installed with the app.
fn resources() -> (PathBuf, PathBuf) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let res = std::env::var("MELFIELD_RESOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|_| exe_dir.clone());
    let uv = exe_dir.join(if cfg!(windows) { "uv.exe" } else { "uv" });
    (res, uv)
}

fn fail(e: MelError) -> ! {
    eprintln!("\n  {}", e.message);
    if std::env::var("MELFIELD_DEBUG").is_ok() {
        eprintln!("  ({})", e.detail);
    }
    std::process::exit(1)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("MELFIELD_LOG").unwrap_or_else(|_| "warn".into()))
        .with_writer(std::io::stderr)
        .init();
    let cli = Cli::parse();
    let (resources_dir, uv_path) = resources();
    let headless = !matches!(&cli.cmd, Cmd::Say { play: true, .. });
    let config = Config {
        root: None,
        resources_dir,
        uv_path,
        app_version: env!("CARGO_PKG_VERSION").into(),
        headless,
    };
    let mf = Melfield::new(config, Arc::new(ConsoleSink)).unwrap_or_else(|e| fail(e));
    match cli.cmd {
        Cmd::List { installed } => {
            for v in mf.voices().unwrap_or_else(|e| fail(e)) {
                if installed && !v.installed {
                    continue;
                }
                let status = if v.installed {
                    "●".to_string()
                } else {
                    format!("○ {}", v.download_label)
                };
                println!("  {:<10} {:<28} {}", v.id, v.meta, status);
            }
        }
        Cmd::Pull { voice } => {
            let id = voice.to_lowercase();
            mf.get_voice(&id).await.unwrap_or_else(|e| fail(e));
            eprintln!("\n  {id} is ready");
        }
        Cmd::Say {
            text,
            voice,
            output,
            speed,
            play,
        } => {
            let voice_id = voice.map(|v| v.to_lowercase()).unwrap_or_else(|| {
                mf.settings()
                    .map(|s| s.default_voice)
                    .unwrap_or_else(|e| fail(e))
            });
            let req = SpeakRequest {
                text,
                voice_id,
                speed,
                delivery: Default::default(),
                language: None,
                play: false,
            };
            let mf2 = mf.clone();
            let (pcm, _) = tokio::task::spawn_blocking(move || {
                mf2.synthesize_blocking(&req, &CancelToken::default())
            })
            .await
            .unwrap_or_else(|e| fail(MelError::internal(e)))
            .unwrap_or_else(|e| fail(e));
            let out =
                output.unwrap_or_else(|| PathBuf::from(format!("melfield-{}.wav", chrono_stamp())));
            let format = out
                .extension()
                .and_then(|e| e.to_str())
                .and_then(Format::parse)
                .unwrap_or(Format::Wav);
            codec::save(&out, &pcm, format).unwrap_or_else(|e| fail(e));
            eprintln!("  saved {} ({:.1}s)", out.display(), pcm.duration_s());
            if play {
                play_and_wait(&mf, &out, pcm.duration_s()).await;
            }
        }
        Cmd::Serve { port, token } => {
            let server = melfield_api::start(mf.clone(), port, token)
                .await
                .unwrap_or_else(|e| fail(e));
            eprintln!("  listening on http://{} — ctrl+c to stop", server.addr);
            let _ = tokio::signal::ctrl_c().await;
            server.stop();
        }
    }
    mf.shutdown();
}

async fn play_and_wait(mf: &Arc<Melfield>, path: &std::path::Path, secs: f64) {
    if let Err(e) = mf.play_file("cli", path) {
        fail(e);
    }
    tokio::time::sleep(std::time::Duration::from_secs_f64(secs + 0.3)).await;
}

fn chrono_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    secs.to_string()
}
