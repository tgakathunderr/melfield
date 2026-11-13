import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

import type { Backend } from "./api";
import type { CoreEvent, Generation } from "./types";

const AUDIO_EXTENSIONS = ["wav", "mp3", "m4a", "flac", "ogg", "aac", "mp4"];

function fileName(g: Generation, format: string): string {
  const words = g.text.trim().split(/\s+/).slice(0, 5).join("-").toLowerCase().replace(/[^a-z0-9\-ऀ-ॿ]/g, "");
  return `${g.voiceName.toLowerCase()}-${words || "clip"}.${format}`;
}

export function createTauriBackend(): Backend {
  let cachedHistory: Generation[] = [];
  return {
    appState: () => invoke("app_state"),
    voices: () => invoke("voices"),
    getVoice: (voiceId) => invoke("get_voice", { voiceId }),
    installRecipe: (recipeId) => invoke("install_recipe", { recipeId }),
    cancelPull: (recipeId) => invoke("cancel_pull", { recipeId }),
    removeVoice: (voiceId) => invoke("remove_voice", { voiceId }),
    voicesSharingFiles: (voiceId) => invoke("voices_sharing_files", { voiceId }),
    acceptLicense: (recipeId) => invoke("accept_license", { recipeId }),
    startSetup: () => invoke("start_setup"),
    speak: (req) => invoke("speak", { req }),
    cancelJob: (jobId) => invoke("cancel_job", { jobId }),
    previewVoice: (voiceId) => invoke("preview_voice", { voiceId }),
    playGeneration: (id) => invoke("play_generation", { id }),
    playerControl: (action, seconds) => invoke("player_control", { action, seconds: seconds ?? null }),
    history: async (query, limit, offset) => {
      cachedHistory = await invoke<Generation[]>("history", { query, limit, offset });
      return cachedHistory;
    },
    deleteGeneration: (id) => invoke("delete_generation", { id }),
    setFavourite: (id, favourite) => invoke("set_favourite", { id, favourite }),
    exportGeneration: async (id, format) => {
      const g = cachedHistory.find((x) => x.id === id);
      const dest = await save({
        defaultPath: g ? fileName(g, format) : `melfield.${format}`,
        filters: [{ name: format.toUpperCase(), extensions: [format] }],
      });
      if (!dest) return null;
      await invoke("export_generation", { id, dest, format });
      return dest;
    },
    clonePlan: (language) => invoke("clone_plan", { language }),
    startRecording: () => invoke("start_recording"),
    stopRecording: () => invoke("stop_recording"),
    importAudio: async () => {
      const picked = await open({ multiple: false, filters: [{ name: "Audio", extensions: AUDIO_EXTENSIONS }] });
      if (!picked || Array.isArray(picked)) return null;
      return invoke("import_audio", { path: picked });
    },
    importAudioPath: (path) => invoke("import_audio", { path }),
    playRecording: (id) => invoke("play_recording", { recordingId: id }),
    createClone: (req) => invoke("create_clone", { req }),
    updateSettings: (settings) => invoke("update_settings", { settings }),
    diskUsage: () => invoke("disk_usage"),
    moveStorage: async () => {
      const dest = await open({ directory: true, multiple: false });
      if (!dest || Array.isArray(dest)) return false;
      await invoke("move_storage", { dest });
      return true;
    },
    exportLogs: async () => {
      const dest = await save({ defaultPath: "melfield-logs.zip", filters: [{ name: "Zip", extensions: ["zip"] }] });
      if (!dest) return false;
      await invoke("export_logs", { dest });
      return true;
    },
    engines: () => invoke("engines"),
    repairEngine: (family) => invoke("repair_engine", { family }),
    apiStatus: () => invoke("api_status"),
    setApiEnabled: (enabled) => invoke("set_api_enabled", { enabled }),
    hfOAuthAvailable: () => invoke("hf_oauth_available"),
    hfOAuthBegin: async () => {
      const url = await invoke<string>("hf_oauth_begin");
      await invoke("open_url", { url });
    },
    hfSignInWithToken: (token) => invoke("hf_sign_in_with_token", { token }),
    hfSignOut: () => invoke("hf_sign_out"),
    hfCheckAccess: (recipeId) => invoke("hf_check_access", { recipeId }),
    openUrl: (url) => invoke("open_url", { url }),
    openDataDir: () => invoke("open_data_dir"),
    revealPath: (path) => invoke("reveal_path", { path }),
    openNotices: () => invoke("open_notices"),
    onFileDrop: async (cb) =>
      getCurrentWebview().onDragDropEvent((e) => {
        const p = e.payload;
        if (p.type === "over" || p.type === "enter") cb([], true);
        else if (p.type === "leave") cb([], false);
        else if (p.type === "drop") cb(p.paths, false);
      }),
    checkForUpdate: async () => {
      try {
        const update = await check();
        return update ? update.version : null;
      } catch {
        return null; // offline or no update feed yet: stay quiet
      }
    },
    installUpdate: async () => {
      const update = await check();
      if (!update) return;
      await update.downloadAndInstall();
      await relaunch();
    },
    onEvent: async (cb) => listen<CoreEvent>("melfield://event", (e) => cb(e.payload)),
  };
}
