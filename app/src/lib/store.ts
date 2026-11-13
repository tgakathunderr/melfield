import { create } from "zustand";

import { call } from "./api";
import { audioBus, pushBands } from "./audioBus";
import type {
  AppState,
  CoreEvent,
  Delivery,
  EnvProgress,
  Generation,
  MelError,
  PullProgress,
  Settings,
  SignInTarget,
  Voice,
} from "./types";

export type Screen = "onboarding" | "studio" | "voices" | "clone" | "settings";

export interface ToastAction {
  label: string;
  run: () => void;
}

export interface Toast {
  id: number;
  kind: "error" | "info";
  error: MelError;
  action?: ToastAction;
}

interface PlayerState {
  id: string | null;
  playing: boolean;
  positionS: number;
  durationS: number;
}

interface Job {
  id: string;
  chunksDone: number;
  chunksTotal: number;
}

interface Store {
  ready: boolean;
  app: AppState | null;
  screen: Screen;
  historyOpen: boolean;
  voices: Voice[];
  selectedVoiceId: string;
  speed: number;
  delivery: Delivery;
  pulls: Record<string, PullProgress>;
  envs: Record<string, EnvProgress>;
  job: Job | null;
  lastGeneration: Generation | null;
  /** Player id of the job that produced lastGeneration (its stream keeps playing). */
  lastJobId: string | null;
  player: PlayerState;
  toasts: Toast[];
  /** A pending Hugging Face sign-in sheet (gated voices / cloning engine). */
  signInFor: SignInTarget | null;
  /** A pending license-accept sheet. */
  licenseFor: Voice | null;
  /** Voice whose clone reveal is playing. */
  revealVoiceId: string | null;

  init(): Promise<void>;
  go(screen: Screen): void;
  setHistoryOpen(open: boolean): void;
  refreshVoices(): Promise<void>;
  refreshApp(): Promise<void>;
  selectVoice(id: string): void;
  setSpeed(v: number): void;
  setDelivery(d: Delivery): void;
  speak(text: string): Promise<void>;
  cancel(): Promise<void>;
  getVoice(voice: Voice): Promise<void>;
  updateSettings(patch: Partial<Settings>): Promise<void>;
  showError(e: MelError): void;
  notify(message: string, action?: ToastAction): void;
  dismissToast(id: number): void;
  setSignInFor(target: SignInTarget | null): void;
  setLicenseFor(v: Voice | null): void;
  setReveal(id: string | null): void;
}

let toastSeq = 1;
let lastPlayerCommit = 0;

export const useStore = create<Store>((set, get) => {
  const handle = (e: CoreEvent) => {
    switch (e.type) {
      case "pull": {
        const p = e.payload;
        set((s) => ({ pulls: { ...s.pulls, [p.recipeId]: p } }));
        if (p.state === "installed" || p.state === "failed" || p.state === "cancelled") {
          window.setTimeout(() => set((s) => {
            const rest = { ...s.pulls };
            delete rest[p.recipeId];
            return { pulls: rest };
          }), 1500);
        }
        return;
      }
      case "env":
        set((s) => ({ envs: { ...s.envs, [e.payload.family]: e.payload } }));
        return;
      case "job": {
        const j = e.payload;
        if (j.state === "started" || j.state === "progress") {
          set({ job: { id: j.jobId, chunksDone: j.chunksDone, chunksTotal: j.chunksTotal } });
        } else {
          set({ job: null });
          if (j.generation) set({ lastGeneration: j.generation, lastJobId: j.jobId });
          if (j.error) get().showError(j.error);
        }
        return;
      }
      case "player": {
        const f = e.payload;
        pushBands(f.bands, f.playing);
        const now = performance.now();
        // Commit to React at ~10 Hz (or immediately on play/stop changes).
        if (f.playing !== get().player.playing || now - lastPlayerCommit > 100) {
          lastPlayerCommit = now;
          set({ player: { id: f.id, playing: f.playing, positionS: f.positionS, durationS: f.durationS } });
        }
        return;
      }
      case "record_level":
        audioBus.micLevel = e.payload.level;
        return;
      case "voices_changed":
        void get().refreshVoices();
        return;
      case "settings_changed":
        void get().refreshApp();
        return;
    }
  };

  return {
    ready: false,
    app: null,
    screen: "studio",
    historyOpen: false,
    voices: [],
    selectedVoiceId: "aria",
    speed: 1,
    delivery: "natural",
    pulls: {},
    envs: {},
    job: null,
    lastGeneration: null,
    lastJobId: null,
    player: { id: null, playing: false, positionS: 0, durationS: 0 },
    toasts: [],
    signInFor: null,
    licenseFor: null,
    revealVoiceId: null,

    async init() {
      await call((b) => b.onEvent(handle));
      const [app, voices] = await Promise.all([call((b) => b.appState()), call((b) => b.voices())]);
      const selected = voices.find((v) => v.id === app.settings.defaultVoice && v.installed)?.id ?? voices.find((v) => v.installed)?.id ?? "aria";
      set({ app, voices, ready: true, screen: app.firstRun ? "onboarding" : "studio", selectedVoiceId: selected });
    },
    go(screen) {
      set({ screen, historyOpen: false });
    },
    setHistoryOpen(open) {
      set({ historyOpen: open });
    },
    async refreshVoices() {
      const voices = await call((b) => b.voices());
      set({ voices });
    },
    async refreshApp() {
      const app = await call((b) => b.appState());
      set({ app });
    },
    selectVoice(id) {
      set({ selectedVoiceId: id });
    },
    setSpeed(v) {
      set({ speed: Math.min(2, Math.max(0.5, v)) });
    },
    setDelivery(d) {
      set({ delivery: d });
    },
    async speak(text) {
      const s = get();
      if (s.job) return;
      try {
        const id = await call((b) => b.speak({ text, voiceId: s.selectedVoiceId, speed: s.speed, delivery: s.delivery, play: true }));
        set({ job: { id, chunksDone: 0, chunksTotal: 0 } });
      } catch (e) {
        get().showError(e as MelError);
      }
    },
    async cancel() {
      const job = get().job;
      if (job) await call((b) => b.cancelJob(job.id));
      await call((b) => b.playerControl("stop"));
      set({ job: null });
    },
    async getVoice(voice) {
      if (voice.license.acceptRequired && !voice.license.accepted) {
        set({ licenseFor: voice });
        return;
      }
      const target: SignInTarget = { recipeId: voice.recipeId, label: voice.name, gateUrl: voice.gateUrl, voiceId: voice.id };
      if (voice.needsSignIn) {
        set({ signInFor: target });
        return;
      }
      try {
        await call((b) => b.getVoice(voice.id));
        get().notify(`${voice.name.toLowerCase()} is ready`, { label: "use", run: () => { get().selectVoice(voice.id); get().go("studio"); } });
      } catch (e) {
        const err = e as MelError;
        if (err.fix?.kind === "sign_in" || err.code === "needs_access") set({ signInFor: target });
        else get().showError(err);
      }
    },
    async updateSettings(patch) {
      const app = get().app;
      if (!app) return;
      try {
        const settings = await call((b) => b.updateSettings({ ...app.settings, ...patch }));
        set({ app: { ...app, settings } });
      } catch (e) {
        get().showError(e as MelError);
      }
    },
    showError(error) {
      if (error.code === "cancelled") return;
      const id = toastSeq++;
      set((s) => ({ toasts: [...s.toasts.filter((t) => t.error.message !== error.message), { id, kind: "error", error }] }));
      window.setTimeout(() => get().dismissToast(id), 9000);
    },
    notify(message, action) {
      const id = toastSeq++;
      const error: MelError = { code: "info", message, fix: null };
      set((s) => ({ toasts: [...s.toasts.filter((t) => t.error.message !== message), { id, kind: "info", error, action }] }));
      window.setTimeout(() => get().dismissToast(id), 5000);
    },
    dismissToast(id) {
      set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
    },
    setSignInFor(target) {
      set({ signInFor: target });
    },
    setLicenseFor(v) {
      set({ licenseFor: v });
    },
    setReveal(id) {
      set({ revealVoiceId: id });
    },
  };
});

export function selectedVoice(s: { voices: Voice[]; selectedVoiceId: string }): Voice | undefined {
  return s.voices.find((v) => v.id === s.selectedVoiceId);
}
