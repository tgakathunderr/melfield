// Typed bridge to the Rust core. In a plain browser (vite dev, Playwright UI
// tests) a mock backend stands in so every screen can be exercised without Tauri.

import type {
  ApiStatus,
  AppState,
  ClonePlan,
  CloneRequest,
  CoreEvent,
  DiskUsage,
  EngineStatus,
  Generation,
  MelError,
  RecordingInfo,
  Settings,
  SpeakRequest,
  Voice,
} from "./types";

export interface Backend {
  appState(): Promise<AppState>;
  voices(): Promise<Voice[]>;
  getVoice(voiceId: string): Promise<void>;
  installRecipe(recipeId: string): Promise<void>;
  cancelPull(recipeId: string): Promise<void>;
  removeVoice(voiceId: string): Promise<number>;
  voicesSharingFiles(voiceId: string): Promise<string[]>;
  acceptLicense(recipeId: string): Promise<void>;
  startSetup(): Promise<void>;
  speak(req: SpeakRequest): Promise<string>;
  cancelJob(jobId: string): Promise<void>;
  previewVoice(voiceId: string): Promise<void>;
  playGeneration(id: string): Promise<void>;
  playerControl(action: "pause" | "resume" | "stop" | "seek", seconds?: number): Promise<void>;
  history(query: string | null, limit: number, offset: number): Promise<Generation[]>;
  deleteGeneration(id: string): Promise<void>;
  setFavourite(id: string, favourite: boolean): Promise<void>;
  /** Returns the saved path, or null when the person cancelled. */
  exportGeneration(id: string, format: "wav" | "flac"): Promise<string | null>;
  clonePlan(language: string): Promise<ClonePlan>;
  startRecording(): Promise<void>;
  stopRecording(): Promise<RecordingInfo>;
  importAudio(): Promise<RecordingInfo | null>;
  importAudioPath(path: string): Promise<RecordingInfo>;
  playRecording(id: string): Promise<void>;
  createClone(req: CloneRequest): Promise<Voice>;
  updateSettings(s: Settings): Promise<Settings>;
  diskUsage(): Promise<DiskUsage>;
  moveStorage(): Promise<boolean>;
  exportLogs(): Promise<boolean>;
  engines(): Promise<EngineStatus[]>;
  repairEngine(family: string): Promise<void>;
  apiStatus(): Promise<ApiStatus>;
  setApiEnabled(enabled: boolean): Promise<ApiStatus>;
  hfOAuthAvailable(): Promise<boolean>;
  hfOAuthBegin(): Promise<void>;
  hfSignInWithToken(token: string): Promise<string>;
  hfSignOut(): Promise<void>;
  hfCheckAccess(recipeId: string): Promise<boolean>;
  openUrl(url: string): Promise<void>;
  openDataDir(): Promise<void>;
  revealPath(path: string): Promise<void>;
  openNotices(): Promise<void>;
  /** Paths dropped onto the window (Tauri) — returns an unsubscribe. */
  onFileDrop(cb: (paths: string[], over: boolean) => void): Promise<() => void>;
  checkForUpdate(): Promise<string | null>;
  installUpdate(): Promise<void>;
  onEvent(cb: (e: CoreEvent) => void): Promise<() => void>;
}

export function isMelError(e: unknown): e is MelError {
  return typeof e === "object" && e !== null && "code" in e && "message" in e;
}

/** Turn anything thrown into a calm, human error. Never a stack trace. */
export function toMelError(e: unknown): MelError {
  if (isMelError(e)) return e;
  return { code: "internal", message: "something went sideways — try that again", fix: { kind: "retry" } };
}

export const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let backendPromise: Promise<Backend> | null = null;

export function backend(): Promise<Backend> {
  if (!backendPromise) {
    backendPromise = inTauri
      ? import("./tauriBackend").then((m) => m.createTauriBackend())
      : import("./mockBackend").then((m) => m.createMockBackend());
  }
  return backendPromise;
}

/** Call the backend and normalise errors. */
export async function call<T>(fn: (b: Backend) => Promise<T>): Promise<T> {
  const b = await backend();
  try {
    return await fn(b);
  } catch (e) {
    throw toMelError(e);
  }
}
