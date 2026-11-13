//! End-to-end: install the starter recipe and speak with a native voice.
//!
//! Runs when either:
//!   MELFIELD_KOKORO_DIR=<dir with model.int8.onnx, voices.bin, tokens.txt>  (offline seed), or
//!   MELFIELD_NET_TESTS=1  (real pull from Hugging Face, ~142 MB)
//! Otherwise it is skipped (prints a note) so plain `cargo test` stays offline.

use std::sync::Arc;

use melfield_core::pull::{file_path, CancelToken};
use melfield_core::service::SpeakRequest;
use melfield_core::{Config, Melfield, NullSink};

fn config(root: &std::path::Path) -> Config {
    Config {
        root: Some(root.to_path_buf()),
        resources_dir: root.join("resources"),
        uv_path: "uv".into(),
        app_version: "test".into(),
        headless: true,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn speaks_with_native_kokoro() {
    let seed = std::env::var("MELFIELD_KOKORO_DIR").ok();
    let net = std::env::var("MELFIELD_NET_TESTS").is_ok();
    if seed.is_none() && !net {
        eprintln!("skipping: set MELFIELD_KOKORO_DIR or MELFIELD_NET_TESTS=1");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mf = Melfield::new(config(dir.path()), Arc::new(NullSink)).unwrap();
    let recipe = melfield_core::registry::Registry::bundled()
        .unwrap()
        .recipe("kokoro-82m")
        .unwrap()
        .clone();
    if let Some(seed) = seed {
        let a = &recipe.artifacts[0];
        for f in &a.files {
            let dest = file_path(mf.paths(), a, f);
            std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
            std::fs::copy(std::path::Path::new(&seed).join(&f.path), dest).unwrap();
        }
    }
    mf.start_setup().await.expect("setup");
    let state = mf.app_state().unwrap();
    assert!(!state.first_run);
    let aria = mf.voice("aria").unwrap();
    assert!(aria.installed);

    let req = SpeakRequest {
        text: "Hello from Melfield. Every voice, on your machine — no limits, no credits.\n\nSecond paragraph, 3:45 pm.".into(),
        voice_id: "aria".into(),
        speed: 1.0,
        delivery: Default::default(),
        language: None,
        play: false,
    };
    let mf2 = mf.clone();
    let (pcm, watermarked) =
        tokio::task::spawn_blocking(move || mf2.synthesize_blocking(&req, &CancelToken::default()))
            .await
            .unwrap()
            .expect("synthesize");
    assert!(!watermarked, "stock voices are not watermarked");
    assert!(
        pcm.duration_s() > 4.0 && pcm.duration_s() < 15.0,
        "duration {}",
        pcm.duration_s()
    );
    assert!(pcm.rms() > 0.02, "rms {}", pcm.rms());
    assert!(pcm.peak() <= 0.9);

    // Unsupported language gives a human error, not a crash.
    let hi = SpeakRequest {
        text: "नमस्ते दुनिया".into(),
        voice_id: "aria".into(),
        speed: 1.0,
        delivery: Default::default(),
        language: None,
        play: false,
    };
    let err = mf
        .synthesize_blocking(&hi, &CancelToken::default())
        .unwrap_err();
    assert!(
        err.message.contains("doesn't speak that language"),
        "{}",
        err.message
    );

    // A full speak job lands in history.
    let job = SpeakRequest {
        text: "Saved to history.".into(),
        voice_id: "mira".into(),
        speed: 1.1,
        delivery: Default::default(),
        language: None,
        play: false,
    };
    mf.speak(job).unwrap();
    for _ in 0..200 {
        if !mf.history(None, 10, 0).unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let hist = mf.history(None, 10, 0).unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].voice_name, "Mira");
    let out = dir.path().join("export.flac");
    mf.export_generation(&hist[0].id, &out, melfield_core::audio::codec::Format::Flac)
        .unwrap();
    assert!(std::fs::metadata(&out).unwrap().len() > 1000);
}
