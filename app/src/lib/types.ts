// Mirrors crates/melfield-core serialized types. Keep in sync with the Rust structs.

export type Rgb = [number, number, number];

export interface Orb {
  c1: Rgb;
  c2: Rgb;
  k: number;
  amp: number;
  ring: boolean;
}

export type FixAction =
  | { kind: "retry" }
  | { kind: "free_space"; needed_bytes: number }
  | { kind: "repair"; family: string }
  | { kind: "sign_in" }
  | { kind: "open_page"; url: string }
  | { kind: "get_voice"; voice_id: string }
  | { kind: "install_recipe"; recipe_id: string }
  | { kind: "accept_license"; recipe_id: string }
  | { kind: "open_settings" };

export interface MelError {
  code: string;
  message: string;
  fix: FixAction | null;
}

export interface LicenseView {
  id: string;
  url: string;
  commercial: boolean;
  personalUse: boolean;
  acceptRequired: boolean;
  accepted: boolean;
  attribution: string | null;
  usagePolicy: string | null;
}

export interface Voice {
  id: string;
  name: string;
  description: string;
  meta: string;
  language: string;
  languages: string[];
  tags: string[];
  orb: Orb;
  kind: "stock" | "clone";
  recipeId: string;
  installed: boolean;
  installing: boolean;
  available: boolean;
  unavailableReason: string | null;
  downloadBytes: number;
  downloadLabel: string;
  needsSignIn: boolean;
  gateUrl: string | null;
  slowHere: boolean;
  license: LicenseView;
  hasSample: boolean;
  canSpeed: boolean;
  delivery: string[];
  tagsSupported: string[];
  starter: boolean;
  engineName: string;
}

export interface Gpu {
  vendor: string;
  name: string;
  vram_mb: number;
}

export interface HardwareInfo {
  platform: string;
  os_version: string;
  cpu_name: string;
  cpu_cores: number;
  ram_mb: number;
  gpus: Gpu[];
  tier: string;
  summary: string;
}

export type Compute = "auto" | "gpu" | "cpu";

export interface Settings {
  defaultVoice: string;
  compute: Compute;
  outputFormat: "wav" | "flac";
  apiEnabled: boolean;
  apiPort: number;
  apiRequireToken: boolean;
  reducedEffects: boolean | null;
  checkUpdates: boolean;
  keepHistory: boolean;
  onboarded: boolean;
}

export interface AppState {
  firstRun: boolean;
  hardware: HardwareInfo;
  settings: Settings;
  appVersion: string;
  hfSignedIn: boolean;
  dataDir: string;
}

export interface Generation {
  id: string;
  voiceId: string;
  voiceName: string;
  text: string;
  durationS: number;
  createdAt: string;
  favourite: boolean;
  watermarked: boolean;
  waveform: number[];
  orb: Orb | null;
}

export type Delivery = "calm" | "natural" | "expressive";

export interface SpeakRequest {
  text: string;
  voiceId: string;
  speed: number;
  delivery: Delivery;
  language?: string | null;
  play: boolean;
}

export type PullState = "checking" | "downloading" | "verifying" | "installed" | "failed" | "cancelled";

export interface PullProgress {
  recipeId: string;
  bytesDone: number;
  bytesTotal: number;
  state: PullState;
}

export interface EnvProgress {
  family: string;
  stage: string;
  detail: string;
}

export type JobState = "started" | "progress" | "done" | "failed" | "cancelled";

export interface JobEvent {
  jobId: string;
  state: JobState;
  chunksDone: number;
  chunksTotal: number;
  generation: Generation | null;
  error: MelError | null;
}

export interface PlayerFrame {
  id: string;
  playing: boolean;
  positionS: number;
  durationS: number;
  streaming: boolean;
  bands: number[];
}

export type CoreEvent =
  | { type: "pull"; payload: PullProgress }
  | { type: "env"; payload: EnvProgress }
  | { type: "job"; payload: JobEvent }
  | { type: "player"; payload: PlayerFrame }
  | { type: "record_level"; payload: { level: number; seconds: number } }
  | { type: "voices_changed" }
  | { type: "settings_changed" };

export interface RecordingInfo {
  id: string;
  durationS: number;
  waveform: number[];
  hint: string | null;
}

export interface ClonePlan {
  language: string;
  supported: boolean;
  reason: string | null;
  recipeId: string | null;
  installed: boolean;
  downloadLabel: string;
  needsSignIn: boolean;
  gateUrl: string | null;
  minSeconds: number;
  idealSeconds: number;
  prompt: string;
}

export interface CloneRequest {
  name: string;
  recordingId: string;
  language: string;
  refText: string | null;
  consent: boolean;
}

export interface DiskUsage {
  voicesBytes: number;
  enginesBytes: number;
  historyBytes: number;
  clonesBytes: number;
  freeBytes: number;
  totalLabel: string;
  location: string;
}

export interface EngineStatus {
  family: string;
  label: string;
  recipes: string[];
  ready: boolean;
  running: boolean;
}

/** What a Hugging Face sign-in is for: names the model page to agree on. */
export interface SignInTarget {
  recipeId: string | null;
  /** Shown in copy, e.g. "Ananya" or "voice cloning". */
  label: string;
  gateUrl: string | null;
  /** Voice to download once access is granted. */
  voiceId: string | null;
}

export interface ApiStatus {
  running: boolean;
  url: string | null;
  token: string | null;
}
