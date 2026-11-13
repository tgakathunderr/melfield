// Browser-only stand-in for the Rust core. Used by `vite dev` outside Tauri and
// by Playwright UI tests. Behaviour mirrors the real core closely enough to
// exercise every screen: pulls progress, speak streams spectrum frames, clones
// require consent.

import type { Backend } from "./api";
import type { AppState, CoreEvent, Generation, MelError, Orb, RecordingInfo, Settings, Voice } from "./types";

type Listener = (e: CoreEvent) => void;

const orb = (c1: [number, number, number], c2: [number, number, number], k: number, amp: number, ring = false): Orb => ({ c1, c2, k, amp, ring });

function stock(id: string, name: string, langs: string[], tags: string[], desc: string, o: Orb, recipeId: string, label: string, starter = false): Voice {
  return {
    id, name, description: desc, meta: `${langs.slice(0, 2).map((l) => l.toUpperCase()).join(" · ")} · ${desc}`,
    language: langs[0], languages: langs, tags, orb: o, kind: "stock", recipeId, installed: false, installing: false,
    available: true, unavailableReason: null, downloadBytes: 142_000_000, downloadLabel: label, needsSignIn: recipeId === "indic-parler-tts", gateUrl: recipeId === "indic-parler-tts" ? "https://huggingface.co/ai4bharat/indic-parler-tts" : null, slowHere: recipeId === "indic-parler-tts",
    license: { id: recipeId === "pocket-tts" ? "CC-BY-4.0" : "Apache-2.0", url: "https://huggingface.co", commercial: true, personalUse: false, acceptRequired: false, accepted: false, attribution: null, usagePolicy: null },
    hasSample: false, canSpeed: recipeId === "kokoro-82m", delivery: [], tagsSupported: [], starter, engineName: recipeId,
  };
}

const VOICES: Voice[] = [
  stock("aria", "Aria", ["en"], ["English"], "warm narrator", orb([255, 170, 90], [224, 56, 154], 4, 0.12), "kokoro-82m", "142 MB", true),
  stock("kabir", "Kabir", ["en"], ["English"], "calm, indian accent", orb([150, 210, 70], [140, 150, 255], 6, 0.07), "kokoro-82m", "142 MB", true),
  stock("mira", "Mira", ["en"], ["English"], "bright, chatty", orb([235, 235, 245], [150, 150, 165], 9, 0.1, true), "kokoro-82m", "142 MB", true),
  stock("theo", "Theo", ["en"], ["English"], "deep, documentary", orb([107, 75, 216], [40, 30, 120], 5, 0.09), "kokoro-82m", "142 MB"),
  stock("ananya", "Ananya", ["hi"], ["Hindi"], "gentle storyteller", orb([255, 120, 150], [255, 106, 43], 7, 0.08, true), "indic-parler-tts", "3.8 GB"),
  stock("rohit", "Rohit", ["hi"], ["Hindi"], "warm, clear", orb([255, 150, 60], [120, 60, 200], 5, 0.1), "indic-parler-tts", "3.8 GB"),
  stock("alba", "Alba", ["en"], ["English"], "soft, close", orb([255, 200, 180], [180, 120, 220], 5, 0.1), "pocket-tts", "251 MB"),
  stock("lucia", "Lucía", ["es", "en"], ["Multilingual"], "lively", orb([255, 210, 80], [255, 90, 60], 4, 0.16), "qwen3-tts-1.7b-customvoice", "4.5 GB"),
];

function err(code: string, message: string, fix: MelError["fix"] = null): MelError {
  return { code, message, fix };
}

export function createMockBackend(): Backend {
  const listeners = new Set<Listener>();
  const opened: string[] = [];
  (window as unknown as { __melfieldOpened: string[] }).__melfieldOpened = opened;
  const emit = (e: CoreEvent) => listeners.forEach((l) => l(e));
  const voices: Voice[] = VOICES.map((v) => ({ ...v }));
  const history: Generation[] = [];
  const recordings = new Map<string, RecordingInfo>();
  let settings: Settings = {
    defaultVoice: "aria", compute: "auto", outputFormat: "wav", apiEnabled: false, apiPort: 5150, apiRequireToken: false,
    reducedEffects: null, checkUpdates: true, keepHistory: true, onboarded: false,
  };
  let signedIn = false;
  let recordStart = 0;
  let recordTimer: number | undefined;
  let playTimer: number | undefined;

  const setInstalled = (recipeId: string, installed: boolean) => {
    voices.forEach((v) => {
      if (v.recipeId === recipeId) {
        v.installed = installed;
        v.installing = false;
        v.downloadLabel = installed ? "" : v.downloadLabel || "142 MB";
      }
    });
    emit({ type: "voices_changed" });
  };

  const pull = (recipeId: string) =>
    new Promise<void>((resolve) => {
      const total = 142_000_000;
      let done = 0;
      voices.forEach((v) => v.recipeId === recipeId && (v.installing = true));
      emit({ type: "voices_changed" });
      const t = window.setInterval(() => {
        done = Math.min(total, done + total / 25);
        emit({ type: "pull", payload: { recipeId, bytesDone: done, bytesTotal: total, state: done >= total ? "installed" : "downloading" } });
        if (done >= total) {
          window.clearInterval(t);
          setInstalled(recipeId, true);
          resolve();
        }
      }, 80);
    });

  const play = (id: string, durationS: number) => {
    window.clearInterval(playTimer);
    const start = performance.now();
    playTimer = window.setInterval(() => {
      const pos = (performance.now() - start) / 1000;
      const playing = pos < durationS;
      const bands = Array.from({ length: 64 }, (_, i) => (playing ? Math.max(0, 0.55 + 0.4 * Math.sin(pos * 9 + i * 0.35) * Math.sin(pos * 3.1 + i * 0.11) - i / 140) : 0));
      emit({ type: "player", payload: { id, playing, positionS: Math.min(pos, durationS), durationS, streaming: false, bands } });
      if (!playing) window.clearInterval(playTimer);
    }, 16);
  };

  const voice = (id: string) => {
    const v = voices.find((x) => x.id === id);
    if (!v) throw err("voice_not_found", "that voice isn't here anymore");
    return v;
  };

  const state = (): AppState => ({
    firstRun: !settings.onboarded,
    hardware: { platform: "browser", os_version: "", cpu_name: "mock", cpu_cores: 8, ram_mb: 16000, gpus: [], tier: "D", summary: "running on your processor — every starter voice works" },
    settings, appVersion: "0.1.0-mock", hfSignedIn: signedIn, dataDir: "/mock/melfield",
  });

  return {
    appState: async () => state(),
    voices: async () => voices.map((v) => ({ ...v })),
    getVoice: async (id) => {
      const v = voice(id);
      if (v.needsSignIn && !signedIn) throw err("needs_sign_in", "this voice comes from hugging face — sign in once (it's free) and it'll download", { kind: "sign_in" });
      await pull(v.recipeId);
    },
    installRecipe: (recipeId) => pull(recipeId),
    cancelPull: async () => undefined,
    removeVoice: async (id) => {
      const v = voice(id);
      if (v.kind === "clone") voices.splice(voices.indexOf(v), 1);
      else setInstalled(v.recipeId, false);
      emit({ type: "voices_changed" });
      return 0;
    },
    voicesSharingFiles: async (id) => voices.filter((v) => v.recipeId === voice(id).recipeId && v.id !== id).map((v) => v.name),
    acceptLicense: async () => undefined,
    startSetup: async () => {
      await pull("kokoro-82m");
      settings = { ...settings, onboarded: true };
    },
    speak: async (req) => {
      const v = voice(req.voiceId);
      if (!req.text.trim()) throw err("empty_text", "type something first");
      if (!v.installed) throw err("voice_not_installed", `${v.name.toLowerCase()} isn't on this computer yet`, { kind: "get_voice", voice_id: v.id });
      const jobId = crypto.randomUUID();
      const durationS = Math.max(1.2, req.text.length / 15 / req.speed);
      window.setTimeout(() => {
        emit({ type: "job", payload: { jobId, state: "started", chunksDone: 0, chunksTotal: 1, generation: null, error: null } });
        play(jobId, durationS);
        const g: Generation = {
          id: crypto.randomUUID(), voiceId: v.id, voiceName: v.name, text: req.text, durationS, createdAt: new Date().toISOString(),
          favourite: false, watermarked: v.kind === "clone", waveform: Array.from({ length: 28 }, (_, i) => Math.abs(Math.sin(i * 0.7) * 0.7 + Math.sin(i * 1.9) * 0.3)), orb: v.orb,
        };
        history.unshift(g);
        window.setTimeout(() => emit({ type: "job", payload: { jobId, state: "done", chunksDone: 1, chunksTotal: 1, generation: g, error: null } }), 300);
      }, 250);
      return jobId;
    },
    cancelJob: async () => window.clearInterval(playTimer),
    previewVoice: async (id) => play(`preview:${id}`, 2),
    playGeneration: async (id) => play(id, history.find((g) => g.id === id)?.durationS ?? 2),
    playerControl: async (action) => {
      if (action === "pause" || action === "stop") window.clearInterval(playTimer);
    },
    history: async (q) => history.filter((g) => !q || g.text.toLowerCase().includes(q.toLowerCase())),
    deleteGeneration: async (id) => {
      const i = history.findIndex((g) => g.id === id);
      if (i >= 0) history.splice(i, 1);
    },
    setFavourite: async (id, fav) => {
      const g = history.find((x) => x.id === id);
      if (g) g.favourite = fav;
    },
    exportGeneration: async (id) => `C:/Users/you/Downloads/${id}.wav`,
    clonePlan: async (language) => ({
      language, supported: true, reason: null, recipeId: "pocket-tts-clone", installed: voices.some((v) => v.recipeId === "kokoro-82m" && v.installed),
      downloadLabel: "470 MB", needsSignIn: false, gateUrl: null, minSeconds: 3, idealSeconds: 10,
      prompt: "Hi! I'm recording my voice for Melfield. I like long walks, warm tea, and stories that end well.",
    }),
    startRecording: async () => {
      recordStart = performance.now();
      recordTimer = window.setInterval(() => {
        const seconds = (performance.now() - recordStart) / 1000;
        emit({ type: "record_level", payload: { level: 0.3 + 0.3 * Math.abs(Math.sin(seconds * 7)), seconds } });
      }, 33);
    },
    stopRecording: async () => {
      window.clearInterval(recordTimer);
      const durationS = (performance.now() - recordStart) / 1000;
      if (durationS < 1) throw err("too_short", "that was a little short — try at least 10 seconds");
      const info: RecordingInfo = { id: crypto.randomUUID(), durationS, waveform: Array.from({ length: 40 }, (_, i) => Math.abs(Math.sin(i * 0.5))), hint: durationS < 8 ? "a few more seconds will sound even better" : null };
      recordings.set(info.id, info);
      return info;
    },
    importAudio: async () => {
      const info: RecordingInfo = { id: crypto.randomUUID(), durationS: 14.2, waveform: Array.from({ length: 40 }, (_, i) => Math.abs(Math.sin(i * 0.4))), hint: null };
      recordings.set(info.id, info);
      return info;
    },
    importAudioPath: async () => {
      const info: RecordingInfo = { id: crypto.randomUUID(), durationS: 12, waveform: [], hint: null };
      recordings.set(info.id, info);
      return info;
    },
    playRecording: async (id) => play(`take:${id}`, recordings.get(id)?.durationS ?? 2),
    createClone: async (req) => {
      if (!req.consent) throw err("consent_required", "turn on the permission switch first");
      if (!req.name.trim()) throw err("unsupported", "give this voice a name");
      const v: Voice = {
        ...stock(`clone-${Date.now()}`, req.name.trim(), [req.language], ["English", "My clones"], "your clone", orb([124, 242, 154], [63, 182, 255], 3, 0.15), "pocket-tts-clone", ""),
        kind: "clone", installed: true,
      };
      voices.unshift(v);
      emit({ type: "voices_changed" });
      return v;
    },
    updateSettings: async (s) => {
      settings = s;
      emit({ type: "settings_changed" });
      return s;
    },
    diskUsage: async () => ({ voicesBytes: 142_000_000, enginesBytes: 0, historyBytes: 2_400_000, clonesBytes: 0, freeBytes: 120_000_000_000, totalLabel: "144 MB", location: "/mock/melfield" }),
    moveStorage: async () => false,
    exportLogs: async () => true,
    engines: async () => [{ family: "native", label: "built-in voices", recipes: ["kokoro-82m"], ready: true, running: false }],
    repairEngine: async () => undefined,
    apiStatus: async () => ({ running: settings.apiEnabled, url: settings.apiEnabled ? `http://127.0.0.1:${settings.apiPort}` : null, token: null }),
    setApiEnabled: async (enabled) => {
      settings = { ...settings, apiEnabled: enabled };
      return { running: enabled, url: enabled ? `http://127.0.0.1:${settings.apiPort}` : null, token: null };
    },
    hfOAuthAvailable: async () => false,
    hfOAuthBegin: async () => undefined,
    hfSignInWithToken: async (token) => {
      if (!token.startsWith("hf_")) throw err("needs_sign_in", "that doesn't look like a hugging face key — it starts with hf_");
      signedIn = true;
      voices.forEach((v) => (v.needsSignIn = false));
      settings = { ...settings };
      emit({ type: "voices_changed" });
      return "you";
    },
    hfSignOut: async () => {
      signedIn = false;
    },
    hfCheckAccess: async () => true,
    // The mock records links instead of opening tabs, so tests can assert on them.
    openUrl: async (url) => {
      opened.push(url);
    },
    openDataDir: async () => {
      opened.push("folder:data");
    },
    revealPath: async (path) => {
      opened.push(`reveal:${path}`);
    },
    openNotices: async () => {
      opened.push("notices");
    },
    onFileDrop: async () => () => undefined,
    checkForUpdate: async () => null,
    installUpdate: async () => undefined,
    onEvent: async (cb) => {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
  };
}
