//! Tauri commands: one-line wrappers over melfield-core. Errors cross the
//! bridge as `MelError` (code + human message + fix), never as strings with traces.

use std::path::PathBuf;

use melfield_core::audio::codec::Format;
use melfield_core::error::{MelError, MelResult};
use melfield_core::service::{
    AppState as CoreAppState, ClonePlan, CloneRequest, DiskUsage, EngineStatus, GenerationView,
    RecordingInfo, Settings, SpeakRequest, VoiceView,
};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::AppState;

type R<T> = Result<T, MelError>;

/// Log every failed command with its name and technical detail (never shown in UI).
fn logged<T>(command: &str, result: R<T>) -> R<T> {
    if let Err(e) = &result {
        tracing::warn!(command, code = ?e.code, detail = %e.detail, message = %e.message, "command failed");
    }
    result
}

#[tauri::command]
pub fn app_state(s: State<AppState>) -> R<CoreAppState> {
    logged("app_state", s.mf.app_state())
}

#[tauri::command]
pub fn voices(s: State<AppState>) -> R<Vec<VoiceView>> {
    logged("voices", s.mf.voices())
}

#[tauri::command]
pub async fn get_voice(s: State<'_, AppState>, voice_id: String) -> R<()> {
    logged("get_voice", s.mf.get_voice(&voice_id).await)
}

#[tauri::command]
pub async fn install_recipe(s: State<'_, AppState>, recipe_id: String) -> R<()> {
    logged("install_recipe", s.mf.install_recipe(&recipe_id).await)
}

#[tauri::command]
pub fn cancel_pull(s: State<AppState>, recipe_id: String) {
    s.mf.cancel_pull(&recipe_id);
}

#[tauri::command]
pub fn remove_voice(s: State<AppState>, voice_id: String) -> R<u64> {
    logged("remove_voice", s.mf.remove_voice(&voice_id))
}

#[tauri::command]
pub fn voices_sharing_files(s: State<AppState>, voice_id: String) -> R<Vec<String>> {
    logged("voices_sharing_files", s.mf.voices_sharing_files(&voice_id))
}

#[tauri::command]
pub fn accept_license(s: State<AppState>, recipe_id: String) -> R<()> {
    logged("accept_license", s.mf.accept_license(&recipe_id))
}

#[tauri::command]
pub async fn start_setup(s: State<'_, AppState>) -> R<()> {
    logged("start_setup", s.mf.start_setup().await)
}

#[tauri::command]
pub fn speak(s: State<AppState>, req: SpeakRequest) -> R<String> {
    logged("speak", s.mf.speak(req))
}

#[tauri::command]
pub fn cancel_job(s: State<AppState>, job_id: String) {
    s.mf.cancel_job(&job_id);
}

#[tauri::command]
pub fn preview_voice(s: State<AppState>, voice_id: String) -> R<()> {
    logged("preview_voice", s.mf.preview_voice(&voice_id))
}

#[tauri::command]
pub fn play_generation(s: State<AppState>, id: String) -> R<()> {
    logged("play_generation", s.mf.play_generation(&id))
}

#[tauri::command]
pub fn player_control(s: State<AppState>, action: String, seconds: Option<f64>) -> R<()> {
    logged("player_control", s.mf.player_control(&action, seconds))
}

#[tauri::command]
pub fn history(
    s: State<AppState>,
    query: Option<String>,
    limit: usize,
    offset: usize,
) -> R<Vec<GenerationView>> {
    logged("history", s.mf.history(query.as_deref(), limit, offset))
}

#[tauri::command]
pub fn delete_generation(s: State<AppState>, id: String) -> R<()> {
    logged("delete_generation", s.mf.delete_generation(&id))
}

#[tauri::command]
pub fn set_favourite(s: State<AppState>, id: String, favourite: bool) -> R<()> {
    logged("set_favourite", s.mf.set_favourite(&id, favourite))
}

#[tauri::command]
pub fn export_generation(s: State<AppState>, id: String, dest: PathBuf, format: String) -> R<()> {
    let format = Format::parse(&format).unwrap_or(Format::Wav);
    s.mf.export_generation(&id, &dest, format)
}

#[tauri::command]
pub fn clone_plan(s: State<AppState>, language: String) -> ClonePlan {
    s.mf.clone_plan(&language)
}

#[tauri::command]
pub fn start_recording(s: State<AppState>) -> R<()> {
    logged("start_recording", s.mf.start_recording())
}

#[tauri::command]
pub fn stop_recording(s: State<AppState>) -> R<RecordingInfo> {
    logged("stop_recording", s.mf.stop_recording())
}

#[tauri::command]
pub fn import_audio(s: State<AppState>, path: PathBuf) -> R<RecordingInfo> {
    logged("import_audio", s.mf.import_audio(&path))
}

#[tauri::command]
pub fn play_recording(s: State<AppState>, recording_id: String) -> R<()> {
    logged("play_recording", s.mf.play_recording(&recording_id))
}

#[tauri::command]
pub fn create_clone(s: State<AppState>, req: CloneRequest) -> R<VoiceView> {
    logged("create_clone", s.mf.create_clone(req))
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    s: State<'_, AppState>,
    settings: Settings,
) -> R<Settings> {
    let before = s.mf.settings()?;
    let after = s.mf.update_settings(settings)?;
    let api_changed =
        before.api_port != after.api_port || before.api_require_token != after.api_require_token;
    if after.api_enabled && api_changed {
        stop_api(&app).await;
        start_api(&app).await?;
    }
    Ok(after)
}

#[tauri::command]
pub fn disk_usage(s: State<AppState>) -> DiskUsage {
    s.mf.disk_usage()
}

#[tauri::command]
pub fn move_storage(s: State<AppState>, dest: PathBuf) -> R<()> {
    logged("move_storage", s.mf.move_storage(&dest))
}

#[tauri::command]
pub fn export_logs(s: State<AppState>, dest: PathBuf) -> R<()> {
    logged("export_logs", s.mf.export_logs(&dest))
}

#[tauri::command]
pub fn engines(s: State<AppState>) -> Vec<EngineStatus> {
    s.mf.engines_status()
}

#[tauri::command]
pub async fn repair_engine(s: State<'_, AppState>, family: String) -> R<()> {
    logged("repair_engine", s.mf.repair_engine(&family).await)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiStatus {
    running: bool,
    url: Option<String>,
    token: Option<String>,
}

fn api_token(app: &AppHandle) -> MelResult<Option<String>> {
    let s = app.state::<AppState>();
    if !s.mf.settings()?.api_require_token {
        return Ok(None);
    }
    // A loopback-only token; regenerated each time the API starts.
    let token: String = (0..32)
        .map(|_| char::from(b'a' + rand::random::<u8>() % 26))
        .collect();
    Ok(Some(format!("mf_{token}")))
}

pub async fn start_api(app: &AppHandle) -> R<ApiStatus> {
    let s = app.state::<AppState>();
    let mut slot = s.api.lock().await;
    if slot.is_none() {
        let port = s.mf.settings()?.api_port;
        let token = api_token(app)?;
        let server = melfield_api::start(s.mf.clone(), port, token.clone()).await?;
        if let Ok(mut t) = API_TOKEN.lock() {
            *t = token;
        }
        *slot = Some(server);
    }
    status_of(&slot)
}

static API_TOKEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

async fn stop_api(app: &AppHandle) {
    let s = app.state::<AppState>();
    let server = s.api.lock().await.take();
    if let Some(server) = server {
        server.stop();
    }
}

fn status_of(slot: &Option<melfield_api::ApiServer>) -> R<ApiStatus> {
    Ok(ApiStatus {
        running: slot.is_some(),
        url: slot.as_ref().map(|s| format!("http://{}", s.addr)),
        token: API_TOKEN
            .lock()
            .map_err(|_| MelError::internal("token lock"))?
            .clone(),
    })
}

#[tauri::command]
pub async fn api_status(s: State<'_, AppState>) -> R<ApiStatus> {
    status_of(&*s.api.lock().await)
}

#[tauri::command]
pub async fn set_api_enabled(
    app: AppHandle,
    s: State<'_, AppState>,
    enabled: bool,
) -> R<ApiStatus> {
    let mut settings = s.mf.settings()?;
    settings.api_enabled = enabled;
    if enabled {
        let status = start_api(&app).await?;
        s.mf.update_settings(settings)?;
        Ok(status)
    } else {
        stop_api(&app).await;
        s.mf.update_settings(settings)?;
        status_of(&None)
    }
}

#[tauri::command]
pub fn hf_oauth_available(s: State<AppState>) -> bool {
    s.mf.hf_oauth_available()
}

#[tauri::command]
pub async fn hf_oauth_begin(s: State<'_, AppState>) -> R<String> {
    Ok(s.mf.hf_oauth_begin().await?.authorize_url)
}

#[tauri::command]
pub async fn hf_sign_in_with_token(s: State<'_, AppState>, token: String) -> R<String> {
    Ok(s.mf.hf_sign_in_with_token(&token).await?.name)
}

#[tauri::command]
pub fn hf_sign_out(s: State<AppState>) -> R<()> {
    logged("hf_sign_out", s.mf.hf_sign_out())
}

#[tauri::command]
pub async fn hf_check_access(s: State<'_, AppState>, recipe_id: String) -> R<bool> {
    logged("hf_check_access", s.mf.hf_check_access(&recipe_id).await)
}

// ---- system: links, folders, files ----
// Done in Rust (not the webview opener API) so they never depend on capability
// scopes, and so every URL is validated in one place.

fn opener_failed(e: impl std::fmt::Display) -> MelError {
    MelError::new(
        melfield_core::error::ErrorCode::Internal,
        "couldn't open that — try again, or check that a browser is installed",
    )
    .with_detail(e)
}

/// Open an https link in the person's browser. Anything else is refused.
#[tauri::command]
pub fn open_url(app: AppHandle, url: String) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = url::Url::parse(&url).map_err(|e| opener_failed(format!("bad url {url}: {e}")))?;
    if parsed.scheme() != "https" {
        return logged(
            "open_url",
            Err(opener_failed(format!("refused non-https url {url}"))),
        );
    }
    logged(
        "open_url",
        app.opener()
            .open_url(parsed.as_str(), None::<&str>)
            .map_err(opener_failed),
    )
}

#[tauri::command]
pub fn open_data_dir(app: AppHandle, s: State<AppState>) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let dir = s.mf.paths().root().to_path_buf();
    logged(
        "open_data_dir",
        app.opener()
            .open_path(dir.to_string_lossy(), None::<&str>)
            .map_err(opener_failed),
    )
}

/// Show a saved file in Explorer / Finder / the file manager.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: PathBuf) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    if !path.exists() {
        return Err(MelError::new(
            melfield_core::error::ErrorCode::NotFound,
            "that file has moved or been deleted",
        ));
    }
    logged(
        "reveal_path",
        app.opener()
            .reveal_item_in_dir(&path)
            .map_err(opener_failed),
    )
}

/// Open the bundled third-party notices (works offline).
#[tauri::command]
pub fn open_notices(app: AppHandle) -> R<()> {
    use tauri_plugin_opener::OpenerExt;
    let candidates = [
        app.path()
            .resource_dir()
            .ok()
            .map(|d| d.join("THIRD_PARTY_NOTICES.md")),
        Some(PathBuf::from("../../THIRD_PARTY_NOTICES.md")),
    ];
    let Some(path) = candidates.into_iter().flatten().find(|p| p.exists()) else {
        return open_url(
            app,
            "https://github.com/tgakathunderr/melfield/blob/main/THIRD_PARTY_NOTICES.md".into(),
        );
    };
    logged(
        "open_notices",
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(opener_failed),
    )
}
