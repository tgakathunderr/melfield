import { useEffect, useRef, useState } from "react";

import { addDrawer } from "../canvas/loop";
import { drawRecordOrb } from "../canvas/recordOrb";
import { formatTime } from "../components/DotWave";
import { OrbCanvas } from "../components/OrbCanvas";
import { Play } from "../components/icons";
import { audioBus } from "../lib/audioBus";
import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { ClonePlan, MelError, RecordingInfo, Voice } from "../lib/types";

const LANGUAGES: [string, string][] = [
  ["en", "english"],
  ["hi", "हिन्दी · hindi"],
  ["es", "español"],
  ["fr", "français"],
  ["de", "deutsch"],
  ["pt", "português"],
  ["it", "italiano"],
  ["ja", "日本語"],
  ["ko", "한국어"],
  ["zh", "中文"],
];

const CONSENT = "This is my voice, or I have the speaker's permission to clone it.";
const AUDIO_EXT = /\.(wav|mp3|m4a|flac|ogg|aac|mp4)$/i;

function Reveal({ voice, onDone }: { voice: Voice; onDone: () => void }) {
  const start = useRef(performance.now());
  const [ready, setReady] = useState(false);
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    useStore.getState().selectVoice(voice.id);
    const t = window.setTimeout(() => setReady(true), 1700);
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && done.current();
    window.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(t);
      window.removeEventListener("keydown", onKey);
    };
  }, [voice.id]);
  return (
    <div className="reveal" role="dialog" aria-modal="true" aria-label={`${voice.name} is ready`}>
      <OrbCanvas orb={voice.orb} seed={7} size={280} assemble={() => Math.min(1, (performance.now() - start.current) / 1600)} pulse={() => audioBus.energy} />
      <h2 className="display lg">hello, {voice.name.toLowerCase()}.</h2>
      <p className="mono md">your voice is ready · watermarked · it never leaves this computer</p>
      {ready && (
        <div style={{ display: "flex", gap: 10 }}>
          <button type="button" className="btn ghost" onClick={() => done.current()}>clone another</button>
          <button
            type="button"
            className="btn lg iri"
            autoFocus
            onClick={() => {
              done.current();
              useStore.getState().go("studio");
            }}
          >
            try it in studio
          </button>
        </div>
      )}
    </div>
  );
}

export function Clone() {
  const orbRef = useRef<HTMLCanvasElement>(null);
  const [lang, setLang] = useState("en");
  const [plan, setPlan] = useState<ClonePlan | null>(null);
  const [recording, setRecording] = useState(false);
  const [seconds, setSeconds] = useState(0);
  const [take, setTake] = useState<RecordingInfo | null>(null);
  const [readPrompt, setReadPrompt] = useState(false);
  const [name, setName] = useState("");
  const [consent, setConsent] = useState(false);
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [created, setCreated] = useState<Voice | null>(null);
  const pull = useStore((s) => (plan?.recipeId ? s.pulls[plan.recipeId] : undefined));
  const recStart = useRef(0);

  const refreshPlan = (l: string) =>
    void call((b) => b.clonePlan(l)).then(setPlan).catch((e: MelError) => useStore.getState().showError(e));
  useEffect(() => refreshPlan(lang), [lang]);
  useEffect(() => {
    if (pull?.state === "installed") refreshPlan(lang);
  }, [pull?.state]);

  useEffect(() => {
    const cv = orbRef.current;
    if (!cv) return;
    return addDrawer((t) => drawRecordOrb(cv, t, audioBus.recording));
  }, [plan?.supported]);

  useEffect(() => {
    if (!recording) return;
    const t = window.setInterval(() => setSeconds((performance.now() - recStart.current) / 1000), 200);
    return () => window.clearInterval(t);
  }, [recording]);

  // Stop the mic if the person leaves the screen mid-recording.
  useEffect(
    () => () => {
      if (audioBus.recording) {
        audioBus.recording = false;
        void call((b) => b.stopRecording()).catch(() => undefined);
      }
    },
    [],
  );

  const importPath = async (path: string) => {
    if (!AUDIO_EXT.test(path)) {
      useStore.getState().showError({ code: "bad_audio", message: "that isn't an audio file — try a wav, mp3, m4a or flac", fix: null });
      return;
    }
    try {
      setTake(await call((b) => b.importAudioPath(path)));
      setReadPrompt(false);
    } catch (e) {
      useStore.getState().showError(e as MelError);
    }
  };

  // Real drag & drop: Tauri gives us file paths for files dropped on the window.
  useEffect(() => {
    let off: (() => void) | undefined;
    void call((b) =>
      b.onFileDrop((paths, over) => {
        setDragging(over);
        if (paths.length > 0) void importPath(paths[0]);
      }),
    ).then((u) => (off = u));
    return () => off?.();
  }, []);

  const toggleRecord = async () => {
    try {
      if (!recording) {
        await call((b) => b.startRecording());
        recStart.current = performance.now();
        audioBus.recording = true;
        setSeconds(0);
        setTake(null);
        setReadPrompt(true);
        setRecording(true);
      } else {
        audioBus.recording = false;
        setRecording(false);
        setTake(await call((b) => b.stopRecording()));
      }
    } catch (e) {
      audioBus.recording = false;
      setRecording(false);
      useStore.getState().showError(e as MelError);
    }
  };

  const pickFile = async () => {
    try {
      const info = await call((b) => b.importAudio());
      if (info) {
        setTake(info);
        setReadPrompt(false);
      }
    } catch (e) {
      useStore.getState().showError(e as MelError);
    }
  };

  const create = async () => {
    if (!take || !plan) return;
    setBusy(true);
    try {
      if (!plan.installed && plan.recipeId) {
        if (plan.needsSignIn) {
          useStore.getState().setSignInFor({ recipeId: plan.recipeId, label: "voice cloning", gateUrl: plan.gateUrl, voiceId: null });
          return;
        }
        await call((b) => b.installRecipe(plan.recipeId as string));
      }
      const voice = await call((b) => b.createClone({ name, recordingId: take.id, language: lang, refText: readPrompt ? plan.prompt : null, consent }));
      setCreated(voice);
      setTake(null);
      setName("");
      setConsent(false);
      setReadPrompt(false);
    } catch (e) {
      const err = e as MelError;
      if ((err.fix?.kind === "sign_in" || err.code === "needs_access") && plan.recipeId) {
        useStore.getState().setSignInFor({ recipeId: plan.recipeId, label: "voice cloning", gateUrl: plan.gateUrl, voiceId: null });
      } else {
        useStore.getState().showError(err);
      }
    } finally {
      setBusy(false);
      refreshPlan(lang);
    }
  };

  const unsupported = plan && !plan.supported;
  const named = name.trim().length > 0;
  const canCreate = consent && !!take && named && !busy && !unsupported;
  const downloading = !!pull && pull.state !== "installed";
  const step = !take ? 1 : !named ? 2 : !consent ? 3 : 4;
  const needed = !take ? "record or drop a clip first" : !named ? "give the voice a name" : !consent ? "turn on the permission switch" : "";

  return (
    <div className="page app-bg-dots">
      <main className="clone-main">
        <h1 className="display lg">lend me your voice</h1>
        <div className="clone-steps" aria-label="Steps">
          {["record", "name it", "permission", "create"].map((label, i) => (
            <span key={label} className={i + 1 < step ? "done" : i + 1 === step ? "on" : ""}>
              {i + 1 < step ? "✓" : i + 1} · {label}
            </span>
          ))}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
          <label className="mono md" htmlFor="clone-lang">i'll speak</label>
          <select id="clone-lang" className="lang-select" value={lang} onChange={(e) => setLang(e.target.value)} disabled={recording}>
            {LANGUAGES.map(([code, label]) => (
              <option key={code} value={code}>{label}</option>
            ))}
          </select>
        </div>
        {unsupported ? (
          <p className="mono md" role="status" style={{ color: "var(--rec-red)" }}>{plan.reason}</p>
        ) : (
          <>
            {plan && (
              <div className="prompt-card" lang={lang} style={{ opacity: recording || readPrompt ? 1 : 0.6 }}>
                <span className="mono" style={{ display: "block", marginBottom: 6 }}>
                  {recording ? "read this out loud, naturally" : "tap the circle, then read this out loud"}
                </span>
                {plan.prompt}
              </div>
            )}
            <button type="button" className="record-orb" aria-pressed={recording} aria-label={recording ? "Stop recording" : "Start recording"} onClick={() => void toggleRecord()}>
              <canvas ref={orbRef} aria-hidden="true" />
            </button>
            <div className="mono md" aria-live="polite" style={{ color: recording ? "var(--rec-red)" : "var(--text-muted)" }}>
              {recording
                ? `● recording · ${formatTime(seconds)} · tap again to stop${seconds >= 10 ? " — sounds great" : ""}`
                : take
                  ? null
                  : "10–30 seconds · quiet room · it never leaves this computer"}
            </div>
            {take && !recording && (
              <div className="take">
                <span>✓ {formatTime(take.durationS)} captured{take.hint ? ` · ${take.hint}` : ""}</span>
                <button type="button" className="btn ghost" style={{ minHeight: 30 }} onClick={() => void call((b) => b.playRecording(take.id))}>
                  <Play size={9} /> listen
                </button>
                <button type="button" className="btn ghost" style={{ minHeight: 30 }} onClick={() => void toggleRecord()}>
                  redo
                </button>
              </div>
            )}
            <button type="button" className="btn ghost" disabled={recording} onClick={() => void pickFile()}>
              or choose an audio file — you can also drop one here
            </button>
            <div style={{ width: "100%", display: "flex", flexDirection: "column", gap: 10, marginTop: 8 }}>
              <label htmlFor="voice-name" className="sr-only">Voice name</label>
              <input id="voice-name" className="name-input" type="text" placeholder="name this voice" maxLength={40} value={name} onChange={(e) => setName(e.target.value)} />
              <div className="consent">
                <span id="consent-label">{CONSENT}</span>
                <button type="button" role="switch" className="switch" aria-checked={consent} aria-labelledby="consent-label" onClick={() => setConsent((c) => !c)}>
                  <span className="knob" />
                </button>
              </div>
            </div>
            {plan && !plan.installed && (
              <p className="mono">
                {downloading
                  ? `getting the cloning engine · ${Math.round(((pull?.bytesDone ?? 0) / Math.max(1, pull?.bytesTotal ?? 1)) * 100)}%`
                  : `your first clone downloads the cloning engine once · ${plan.downloadLabel}${plan.needsSignIn ? " · needs a free hugging face sign-in (we'll walk you through it)" : ""}`}
              </p>
            )}
            <button type="button" className="btn lg iri" disabled={!canCreate} onClick={() => void create()}>
              {busy ? (downloading ? "downloading…" : "creating…") : "Create voice"}
            </button>
            {!canCreate && !busy && needed && <p className="mono" aria-live="polite">{needed}</p>}
          </>
        )}
      </main>
      {dragging && (
        <div className="drop-overlay" aria-hidden="true">
          <div>drop to use this recording</div>
        </div>
      )}
      {created && <Reveal voice={created} onDone={() => setCreated(null)} />}
    </div>
  );
}
