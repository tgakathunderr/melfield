//! Pull manager: resumable, checksummed, atomic downloads from the original
//! source. Files land in a Hugging Face–style cache
//! (`models/hf/models--org--name/snapshots/<rev>/<path>`) so Python engines can
//! load them offline with `HF_HUB_OFFLINE=1` and native engines read the same paths.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::error::{ErrorCode, FixAction, MelError, MelResult};
use crate::paths::Paths;
use crate::registry::{Artifact, ArtifactFile, Recipe};

const RETRIES: u32 = 4;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);
/// Headroom on top of the download size for extraction/temp files.
const DISK_HEADROOM: u64 = 200_000_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PullProgress {
    pub recipe_id: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub state: PullState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PullState {
    Checking,
    Downloading,
    Verifying,
    Installed,
    Failed,
    Cancelled,
}

pub type ProgressFn = Arc<dyn Fn(PullProgress) + Send + Sync>;

/// Shared cancel flag for one pull.
#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

pub fn hf_cache_root(paths: &Paths) -> PathBuf {
    paths.models().join("hf")
}

pub fn snapshot_dir(paths: &Paths, artifact: &Artifact) -> PathBuf {
    let safe: String = artifact
        .repo
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect();
    let repo_dir = format!(
        "models--{}",
        if artifact.repo.contains("://") {
            safe
        } else {
            artifact.repo.replace('/', "--")
        }
    );
    hf_cache_root(paths)
        .join(repo_dir)
        .join("snapshots")
        .join(&artifact.revision)
}

pub fn file_path(paths: &Paths, artifact: &Artifact, file: &ArtifactFile) -> PathBuf {
    let mut p = snapshot_dir(paths, artifact);
    for part in file.path.split('/') {
        p.push(part);
    }
    p
}

/// Bytes still to download for this recipe (files already present are skipped).
pub fn remaining_bytes(paths: &Paths, recipe: &Recipe) -> u64 {
    recipe
        .artifacts
        .iter()
        .flat_map(|a| a.files.iter().map(move |f| (a, f)))
        .filter(|(a, f)| !is_complete(&file_path(paths, a, f), f))
        .map(|(_, f)| f.size)
        .sum()
}

fn is_complete(path: &Path, file: &ArtifactFile) -> bool {
    std::fs::metadata(path)
        .map(|m| m.len() == file.size)
        .unwrap_or(false)
}

pub fn is_installed(paths: &Paths, recipe: &Recipe) -> bool {
    remaining_bytes(paths, recipe) == 0
}

pub struct Puller {
    client: reqwest::Client,
    paths: Paths,
}

impl Puller {
    pub fn new(client: reqwest::Client, paths: Paths) -> Self {
        Self { client, paths }
    }

    pub async fn pull(
        &self,
        recipe: &Recipe,
        hf_token: Option<&str>,
        cancel: &CancelToken,
        progress: ProgressFn,
    ) -> MelResult<()> {
        let total = recipe.download_bytes.max(1);
        let report = |done: u64, state: PullState| {
            progress(PullProgress {
                recipe_id: recipe.id.clone(),
                bytes_done: done.min(total),
                bytes_total: total,
                state,
            })
        };
        report(0, PullState::Checking);
        let remaining = remaining_bytes(&self.paths, recipe);
        let free = Paths::free_space(self.paths.models().as_path());
        if free < remaining + DISK_HEADROOM {
            report(0, PullState::Failed);
            return Err(MelError::disk_full(remaining + DISK_HEADROOM - free));
        }
        if recipe.artifacts.iter().any(|a| a.gated) && hf_token.is_none() {
            report(0, PullState::Failed);
            return Err(needs_sign_in());
        }

        let mut done_before = total - remaining.min(total);
        for artifact in &recipe.artifacts {
            for file in &artifact.files {
                let dest = file_path(&self.paths, artifact, file);
                if is_complete(&dest, file) {
                    continue;
                }
                let base = done_before;
                let on_bytes = |n: u64| report(base + n, PullState::Downloading);
                let result = self
                    .fetch_with_retries(artifact, file, &dest, hf_token, cancel, &on_bytes)
                    .await;
                if let Err(e) = result {
                    report(
                        done_before,
                        if e.code == ErrorCode::Cancelled {
                            PullState::Cancelled
                        } else {
                            PullState::Failed
                        },
                    );
                    return Err(e);
                }
                done_before += file.size;
            }
        }
        report(total, PullState::Installed);
        Ok(())
    }

    async fn fetch_with_retries(
        &self,
        artifact: &Artifact,
        file: &ArtifactFile,
        dest: &Path,
        token: Option<&str>,
        cancel: &CancelToken,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> MelResult<()> {
        let mut attempt = 0;
        loop {
            match self
                .fetch_once(artifact, file, dest, token, cancel, on_bytes)
                .await
            {
                Ok(()) => return Ok(()),
                Err(e) if is_retryable(&e) && attempt + 1 < RETRIES => {
                    attempt += 1;
                    tracing::warn!(file = %file.path, attempt, detail = %e.detail, "download retry");
                    tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                }
                Err(e) => return Err(e),
            }
        }
    }

    async fn fetch_once(
        &self,
        artifact: &Artifact,
        file: &ArtifactFile,
        dest: &Path,
        token: Option<&str>,
        cancel: &CancelToken,
        on_bytes: &(dyn Fn(u64) + Sync),
    ) -> MelResult<()> {
        let part = part_path(&self.paths, artifact, file);
        let mut hasher = Sha256::new();
        let mut have = hash_existing(&part, &mut hasher, file.size)?;

        let mut req = self.client.get(artifact.url(file));
        if let Some(t) = token {
            req = req.bearer_auth(t);
        }
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| MelError::offline().with_detail(e))?;
        let status = resp.status();
        match status.as_u16() {
            200 => {
                if have > 0 {
                    // Server ignored the range: start over.
                    hasher = Sha256::new();
                    have = 0;
                }
            }
            206 => {}
            401 => return Err(needs_sign_in()),
            403 => return Err(needs_access(artifact)),
            404 => return Err(download_failed().with_detail(format!("404 {}", artifact.url(file)))),
            416 => {
                // Our partial file is already complete/longer; restart cleanly.
                let _ = std::fs::remove_file(&part);
                return Err(download_failed().with_detail("416 range not satisfiable"));
            }
            _ => return Err(download_failed().with_detail(format!("http {status}"))),
        }

        let mut out = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(have > 0)
            .truncate(have == 0)
            .open(&part)
            .await?;
        let mut stream = resp.bytes_stream();
        let mut last = Instant::now();
        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                out.flush().await?;
                return Err(MelError::cancelled());
            }
            let chunk = chunk.map_err(|e| download_failed().with_detail(e))?;
            out.write_all(&chunk).await?;
            hasher.update(&chunk);
            have += chunk.len() as u64;
            if last.elapsed() >= PROGRESS_EVERY {
                on_bytes(have);
                last = Instant::now();
            }
        }
        out.flush().await?;
        drop(out);
        on_bytes(have);

        if have != file.size {
            return Err(download_failed().with_detail(format!("size {have} != {}", file.size)));
        }
        if let Some(expected) = &file.sha256 {
            let got = hex::encode(hasher.finalize());
            if &got != expected {
                let _ = std::fs::remove_file(&part);
                return Err(MelError::new(
                    ErrorCode::ChecksumMismatch,
                    "a download came through damaged — trying again usually fixes it",
                )
                .with_fix(FixAction::Retry)
                .with_detail(format!("{}: sha256 {got} != {expected}", file.path)));
            }
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&part, dest)?;
        let _ = std::fs::remove_file(part.with_extension("part.json"));
        Ok(())
    }
}

fn part_path(paths: &Paths, artifact: &Artifact, file: &ArtifactFile) -> PathBuf {
    let key = file.sha256.clone().unwrap_or_else(|| {
        hex::encode(Sha256::digest(format!(
            "{}@{}/{}",
            artifact.repo, artifact.revision, file.path
        )))
    });
    paths
        .tmp()
        .join(format!("{}.part", &key[..32.min(key.len())]))
}

/// Feed an existing partial file into the hasher so resume keeps the checksum honest.
fn hash_existing(part: &Path, hasher: &mut Sha256, expected: u64) -> MelResult<u64> {
    let Ok(meta) = std::fs::metadata(part) else {
        return Ok(0);
    };
    if meta.len() > expected {
        std::fs::remove_file(part)?;
        return Ok(0);
    }
    let mut f = std::fs::File::open(part)?;
    let n = std::io::copy(&mut f, hasher)?;
    Ok(n)
}

fn is_retryable(e: &MelError) -> bool {
    matches!(
        e.code,
        ErrorCode::Offline | ErrorCode::DownloadFailed | ErrorCode::ChecksumMismatch
    )
}

fn download_failed() -> MelError {
    MelError::new(
        ErrorCode::DownloadFailed,
        "the download hiccuped — it'll pick up where it left off",
    )
    .with_fix(FixAction::Retry)
}

pub fn needs_sign_in() -> MelError {
    MelError::new(
        ErrorCode::NeedsSignIn,
        "this voice comes from hugging face — sign in once (it's free) and it'll download",
    )
    .with_fix(FixAction::SignIn)
}

pub fn needs_access(artifact: &Artifact) -> MelError {
    let url = format!("https://huggingface.co/{}", artifact.repo);
    MelError::new(
        ErrorCode::NeedsAccess,
        "one more step — tap agree on the model's page, then come back. we'll notice automatically",
    )
    .with_fix(FixAction::OpenPage { url })
}

/// Remove a recipe's files (shared snapshot dirs are only removed when no
/// other installed recipe uses the same repo+revision).
pub fn remove(paths: &Paths, recipe: &Recipe, keep: &[&Recipe]) -> MelResult<u64> {
    let mut freed = 0;
    for artifact in &recipe.artifacts {
        let shared = keep.iter().filter(|r| r.id != recipe.id).any(|r| {
            r.artifacts
                .iter()
                .any(|a| a.repo == artifact.repo && a.revision == artifact.revision)
        });
        if shared {
            continue;
        }
        for file in &artifact.files {
            let p = file_path(paths, artifact, file);
            if let Ok(m) = std::fs::metadata(&p) {
                freed += m.len();
                std::fs::remove_file(&p)?;
            }
        }
    }
    Ok(freed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::Response,
        routing::get,
        Router,
    };
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicUsize;

    #[derive(Clone)]
    struct Server {
        data: Arc<Vec<u8>>,
        hits: Arc<AtomicUsize>,
        /// Cut the connection after this many bytes on the first hit.
        cut_first_at: Option<usize>,
        require_token: bool,
    }

    async fn serve(State(s): State<Server>, headers: HeaderMap) -> Response {
        let hit = s.hits.fetch_add(1, Ordering::SeqCst);
        if s.require_token && headers.get("authorization").is_none() {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::empty())
                .unwrap();
        }
        let start = headers
            .get("range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("bytes="))
            .and_then(|v| v.trim_end_matches('-').parse::<usize>().ok())
            .unwrap_or(0);
        let mut body = s.data[start..].to_vec();
        if hit == 0 {
            if let Some(cut) = s.cut_first_at {
                body.truncate(cut);
            }
        }
        let status = if start > 0 {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        };
        Response::builder()
            .status(status)
            .body(Body::from(body))
            .unwrap()
    }

    async fn start(server: Server) -> String {
        let app = Router::new()
            .route("/{*path}", get(serve))
            .with_state(server);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        format!("http://{addr}")
    }

    fn recipe_for(base: &str, data: &[u8], sha: Option<String>, gated: bool) -> (Recipe, Artifact) {
        let mut r: Recipe = Registry::bundled_recipe();
        let artifact = Artifact {
            source: "test".into(),
            repo: format!("{base}/org/name"),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            gated,
            files: vec![ArtifactFile {
                path: "dir/model.bin".into(),
                size: data.len() as u64,
                sha256: sha,
            }],
        };
        r.artifacts = vec![artifact.clone()];
        r.download_bytes = data.len() as u64;
        (r, artifact)
    }

    use crate::registry::Registry;
    impl Registry {
        fn bundled_recipe() -> Recipe {
            Registry::bundled()
                .unwrap()
                .recipe("kokoro-82m")
                .unwrap()
                .clone()
        }
    }

    /// Tests point artifacts at a local server by overriding the URL builder.
    struct TestPuller(Puller);
    impl TestPuller {
        async fn pull(
            &self,
            r: &Recipe,
            token: Option<&str>,
        ) -> (MelResult<()>, Vec<PullProgress>) {
            let seen = Arc::new(Mutex::new(Vec::new()));
            let s2 = seen.clone();
            let res = self
                .0
                .pull(
                    r,
                    token,
                    &CancelToken::default(),
                    Arc::new(move |p| s2.lock().push(p)),
                )
                .await;
            let v = seen.lock().clone();
            (res, v)
        }
    }

    fn data() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("mf")).unwrap();
        (dir, paths)
    }

    #[tokio::test]
    async fn downloads_verifies_and_installs() {
        let d = data();
        let sha = hex::encode(Sha256::digest(&d));
        let base = start(Server {
            data: Arc::new(d.clone()),
            hits: Default::default(),
            cut_first_at: None,
            require_token: false,
        })
        .await;
        let (_t, paths) = setup();
        let (r, a) = recipe_for(&base, &d, Some(sha), false);
        let p = TestPuller(Puller::new(reqwest::Client::new(), paths.clone()));
        let (res, progress) = p.pull(&r, None).await;
        res.unwrap();
        assert_eq!(
            std::fs::read(file_path(&paths, &a, &a.files[0])).unwrap(),
            d
        );
        assert_eq!(progress.last().unwrap().state, PullState::Installed);
        assert!(is_installed(&paths, &r));
    }

    #[tokio::test]
    async fn resumes_after_connection_drop() {
        let d = data();
        let sha = hex::encode(Sha256::digest(&d));
        let hits = Arc::new(AtomicUsize::new(0));
        let base = start(Server {
            data: Arc::new(d.clone()),
            hits: hits.clone(),
            cut_first_at: Some(100_000),
            require_token: false,
        })
        .await;
        let (_t, paths) = setup();
        let (r, a) = recipe_for(&base, &d, Some(sha), false);
        let p = TestPuller(Puller::new(reqwest::Client::new(), paths.clone()));
        let (res, _) = p.pull(&r, None).await;
        res.unwrap();
        assert!(
            hits.load(Ordering::SeqCst) >= 2,
            "expected a resumed request"
        );
        assert_eq!(
            std::fs::read(file_path(&paths, &a, &a.files[0])).unwrap(),
            d
        );
    }

    #[tokio::test]
    async fn rejects_corrupt_download() {
        let d = data();
        let base = start(Server {
            data: Arc::new(d.clone()),
            hits: Default::default(),
            cut_first_at: None,
            require_token: false,
        })
        .await;
        let (_t, paths) = setup();
        let (r, a) = recipe_for(&base, &d, Some("00".repeat(32)), false);
        let p = TestPuller(Puller::new(reqwest::Client::new(), paths.clone()));
        let (res, progress) = p.pull(&r, None).await;
        assert_eq!(res.unwrap_err().code, ErrorCode::ChecksumMismatch);
        assert!(!file_path(&paths, &a, &a.files[0]).exists());
        assert_eq!(progress.last().unwrap().state, PullState::Failed);
    }

    #[tokio::test]
    async fn gated_without_token_asks_to_sign_in() {
        let d = data();
        let base = start(Server {
            data: Arc::new(d.clone()),
            hits: Default::default(),
            cut_first_at: None,
            require_token: true,
        })
        .await;
        let (_t, paths) = setup();
        let (r, _) = recipe_for(&base, &d, None, true);
        let p = TestPuller(Puller::new(reqwest::Client::new(), paths.clone()));
        let (res, _) = p.pull(&r, None).await;
        assert_eq!(res.unwrap_err().code, ErrorCode::NeedsSignIn);
        let (res, _) = p.pull(&r, Some("hf_test")).await;
        res.unwrap();
    }

    #[test]
    fn layout_matches_hf_cache() {
        let (_t, paths) = setup();
        let a = Artifact {
            source: "huggingface".into(),
            repo: "org/name".into(),
            revision: "abc".into(),
            gated: false,
            files: vec![],
        };
        let f = ArtifactFile {
            path: "a/b.onnx".into(),
            size: 1,
            sha256: None,
        };
        let p = file_path(&paths, &a, &f);
        assert!(p.ends_with(
            Path::new("models--org--name")
                .join("snapshots")
                .join("abc")
                .join("a")
                .join("b.onnx")
        ));
    }
}
