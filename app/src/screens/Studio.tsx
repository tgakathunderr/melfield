import { useEffect, useMemo, useRef, useState } from "react";

import { drawDotField } from "../canvas/dotField";
import { addDrawer } from "../canvas/loop";
import { orbGradient } from "../canvas/orb";
import { DotWave, formatTime } from "../components/DotWave";
import { Arrow, Chevron, Download, Pause, Play, Sliders, Stop } from "../components/icons";
import { saveGeneration } from "../lib/actions";
import { call } from "../lib/api";
import { selectedVoice, useStore } from "../lib/store";
import type { Delivery, MelError, Voice } from "../lib/types";

const DRAFT_KEY = "melfield.draft";

const IDEAS: { label: string; text: string }[] = [
  { label: "welcome line", text: "Hey, welcome back to the channel. Grab a coffee — today's a good one." },
  { label: "story opener", text: "The lighthouse had been dark for forty years. Then, one stormy night in October, it blinked." },
  { label: "ad read", text: "Introducing the quietest blender you'll ever own. Smoothies at sunrise, and nobody wakes up." },
  { label: "podcast intro", text: "You're listening to Small Wonders, the show about tiny things that changed everything." },
];

function useReducedEffects(): boolean {
  const pref = useStore((s) => s.app?.settings.reducedEffects);
  const platform = useStore((s) => s.app?.hardware.platform ?? "");
  return pref ?? platform.startsWith("linux");
}

function MiniOrb({ voice }: { voice: Voice }) {
  return (
    <span
      className="mini-orb"
      aria-hidden="true"
      style={{ backgroundImage: orbGradient(voice.orb), backgroundColor: `rgba(${voice.orb.c1.join(",")},0.12)` }}
    />
  );
}

function usePopover(onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close.current();
    const onDown = (e: MouseEvent) => {
      const el = ref.current;
      const t = e.target as HTMLElement;
      // Ignore clicks on the button that toggles this popover (it handles itself).
      if (el && !el.contains(t) && !t.closest("[data-popover-toggle]")) close.current();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, []);
  return ref;
}

function VoicePicker({ onClose }: { onClose: () => void }) {
  const allVoices = useStore((s) => s.voices);
  const voices = useMemo(() => allVoices.filter((v) => v.installed), [allVoices]);
  const selected = useStore((s) => s.selectedVoiceId);
  const select = useStore((s) => s.selectVoice);
  const go = useStore((s) => s.go);
  const playerId = useStore((s) => (s.player.playing ? s.player.id : null));
  const ref = usePopover(onClose);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("[aria-selected='true']")?.focus();
  }, [ref]);

  const choose = (v: Voice) => {
    select(v.id);
    onClose();
    document.getElementById("script")?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = Array.from(ref.current?.querySelectorAll<HTMLElement>("[role='option']") ?? []);
    const i = items.indexOf(document.activeElement as HTMLElement);
    const next = items[(i + (e.key === "ArrowDown" ? 1 : -1) + items.length) % items.length];
    next?.focus();
  };

  return (
    <div className="popover picker" role="listbox" aria-label="Choose a voice" ref={ref} onKeyDown={onKeyDown}>
      <div className="mono popover-label">your voices</div>
      {voices.map((v) => {
        const previewing = playerId === `preview:${v.id}`;
        return (
          <div key={v.id} className="picker-row">
            <button type="button" role="option" aria-selected={v.id === selected} className="popover-item" onClick={() => choose(v)}>
              <MiniOrb voice={v} />
              <span className="name">{v.name}</span>
              <span className="mono meta">{v.meta}</span>
              {v.id === selected && <span className="check" aria-hidden="true">✓</span>}
            </button>
            <button
              type="button"
              className="icon-btn"
              aria-label={`Hear ${v.name}`}
              onClick={() => void call((b) => b.previewVoice(v.id)).catch((e: MelError) => useStore.getState().showError(e))}
            >
              {previewing ? <Pause size={10} /> : <Play size={10} />}
            </button>
          </div>
        );
      })}
      <button type="button" className="popover-item more" onClick={() => go("voices")}>
        <span className="mono md">browse all voices →</span>
      </button>
    </div>
  );
}

function TuningPopover({ voice, onClose }: { voice: Voice | undefined; onClose: () => void }) {
  const speed = useStore((s) => s.speed);
  const delivery = useStore((s) => s.delivery);
  const setSpeed = useStore((s) => s.setSpeed);
  const setDelivery = useStore((s) => s.setDelivery);
  const ref = usePopover(onClose);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("input, button")?.focus();
  }, [ref]);
  const canSpeed = voice?.canSpeed ?? false;
  const deliveries = (voice?.delivery ?? []) as Delivery[];
  return (
    <div className="popover tuning" ref={ref} role="dialog" aria-label="Voice settings">
      {canSpeed && (
        <div className="slider-row">
          <label className="mono md" htmlFor="speed">speed</label>
          <input id="speed" type="range" min={0.5} max={2} step={0.05} value={speed} onChange={(e) => setSpeed(Number(e.target.value))} />
          <button type="button" className="mono md reset" onClick={() => setSpeed(1)} aria-label="Reset speed">{speed.toFixed(2)}×</button>
        </div>
      )}
      {deliveries.length > 0 && (
        <div style={{ padding: "6px 10px 10px" }}>
          <div className="mono md popover-label">delivery</div>
          <div className="glass-pill" role="group" aria-label="Delivery">
            {deliveries.map((d) => (
              <button key={d} type="button" className="seg" aria-pressed={delivery === d} onClick={() => setDelivery(d)}>
                {d}
              </button>
            ))}
          </div>
        </div>
      )}
      {!canSpeed && deliveries.length === 0 && (
        <p className="mono md" style={{ padding: 12, margin: 0 }}>
          {voice?.name ?? "this voice"} speaks one way — pick another voice for speed or delivery controls
        </p>
      )}
      {voice && voice.tagsSupported.length > 0 && (
        <p className="mono" style={{ padding: "0 12px 10px", margin: 0 }}>
          try {voice.tagsSupported.slice(0, 4).join(" ")} in your text
        </p>
      )}
    </div>
  );
}

function ResultPill() {
  const g = useStore((s) => s.lastGeneration);
  const player = useStore((s) => s.player);
  const job = useStore((s) => s.job);
  const lastJobId = useStore((s) => s.lastJobId);
  const [saving, setSaving] = useState(false);
  if (job) {
    const pct = job.chunksTotal > 0 ? job.chunksDone / job.chunksTotal : 0;
    return (
      <div className="result-pill working-pill" aria-live="polite">
        <button type="button" className="play-btn" aria-label="Stop" onClick={() => void useStore.getState().cancel()}>
          <Stop />
        </button>
        <span className="working">
          <span className="pulse-dots" aria-hidden="true"><i /><i /><i /></span>
          {job.chunksTotal > 1 ? `speaking · part ${Math.min(job.chunksDone + 1, job.chunksTotal)} of ${job.chunksTotal}` : "speaking…"}
        </span>
        {job.chunksTotal > 1 && <span className="mini-bar" aria-hidden="true"><i style={{ width: `${pct * 100}%` }} /></span>}
      </div>
    );
  }
  if (!g) return null;
  const isThis = player.id === g.id || (player.id !== null && player.id === lastJobId);
  const playing = isThis && player.playing;
  const progress = isThis && player.durationS > 0 ? player.positionS / player.durationS : 0;
  const toggle = () => {
    if (playing) void call((b) => b.playerControl("pause"));
    else if (isThis && progress > 0.01 && progress < 0.99) void call((b) => b.playerControl("resume"));
    else void call((b) => b.playGeneration(g.id)).catch((e: MelError) => useStore.getState().showError(e));
  };
  return (
    <div className="result-pill">
      <button type="button" className="play-btn" aria-label={playing ? "Pause" : "Play clip"} onClick={toggle}>
        {playing ? <Pause /> : <Play />}
      </button>
      <DotWave values={g.waveform} progress={progress} />
      <span className="mono" style={{ color: "var(--text-muted)" }}>
        {g.voiceName.toLowerCase()} · {formatTime(g.durationS)}
      </span>
      <button
        type="button"
        className="icon-btn outline"
        aria-label="Save clip"
        title="save (ctrl+s)"
        disabled={saving}
        onClick={() => {
          setSaving(true);
          void saveGeneration(g.id).finally(() => setSaving(false));
        }}
      >
        <Download />
      </button>
    </div>
  );
}

function NoVoiceYet() {
  const go = useStore((s) => s.go);
  return (
    <button type="button" className="voice-chip" onClick={() => go("voices")}>
      get a voice
      <Chevron />
    </button>
  );
}

export function Studio() {
  const canvas = useRef<HTMLCanvasElement>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const reduced = useReducedEffects();
  const voice = useStore(selectedVoice);
  const job = useStore((s) => s.job);
  const speak = useStore((s) => s.speak);
  const hasGeneration = useStore((s) => s.lastGeneration !== null);
  const [text, setText] = useState(() => {
    try {
      return localStorage.getItem(DRAFT_KEY) ?? "";
    } catch {
      return "";
    }
  });
  const [picker, setPicker] = useState(false);
  const [tuning, setTuning] = useState(false);

  useEffect(() => {
    try {
      localStorage.setItem(DRAFT_KEY, text);
    } catch {
      /* storage unavailable: drafts just aren't kept */
    }
  }, [text]);

  useEffect(() => {
    const cv = canvas.current;
    if (!cv) return;
    return addDrawer((t) => drawDotField(cv, t, { intensity: 0.8, reduced }));
  }, [reduced]);

  useEffect(() => {
    textarea.current?.focus();
    const onReopen = (e: Event) => {
      setText((e as CustomEvent<string>).detail);
      textarea.current?.focus();
    };
    window.addEventListener("melfield:reopen", onReopen);
    return () => window.removeEventListener("melfield:reopen", onReopen);
  }, []);

  // Grow the composer with its content.
  useEffect(() => {
    const ta = textarea.current;
    if (!ta) return;
    ta.style.height = "auto";
    ta.style.height = `${Math.min(ta.scrollHeight, window.innerHeight * 0.4)}px`;
  }, [text]);

  // Shortcuts: Space play/pause (when not typing or on a control), Ctrl/⌘+S save, Esc stop.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const s = useStore.getState();
      const onControl = (e.target as HTMLElement)?.closest?.("textarea, input, select, button, [role='switch'], [role='option']");
      if (e.key === "Escape" && s.job) {
        void s.cancel();
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s" && s.lastGeneration) {
        e.preventDefault();
        void saveGeneration(s.lastGeneration.id);
      } else if (e.key === " " && !onControl && s.lastGeneration) {
        e.preventDefault();
        const g = s.lastGeneration;
        void call((b) => (s.player.playing ? b.playerControl("pause") : b.playGeneration(g.id)));
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const submit = () => {
    if (!text.trim() || job) return;
    void speak(text);
  };

  const fillIdea = (idea: string) => {
    setText(idea);
    textarea.current?.focus();
  };

  const chars = text.length;
  const showIdeas = !text.trim() && !hasGeneration && !job;

  return (
    <>
      <canvas ref={canvas} className="field" aria-hidden="true" />
      <main className="studio-main">
        <h1 className="display xl">what should I say?</h1>
        <div className={`composer${job ? " busy" : ""}`}>
          <label htmlFor="script" className="sr-only">Text to speak</label>
          <textarea
            id="script"
            ref={textarea}
            rows={3}
            value={text}
            placeholder="Type or paste anything. A line, a script, a whole chapter…"
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
                e.preventDefault();
                submit();
              }
            }}
          />
          <div className="composer-bar">
            {voice?.installed ? (
              <button type="button" className="voice-chip" data-popover-toggle aria-haspopup="listbox" aria-expanded={picker} onClick={() => { setTuning(false); setPicker((p) => !p); }}>
                <MiniOrb voice={voice} />
                {voice.name}
                <Chevron />
              </button>
            ) : (
              <NoVoiceYet />
            )}
            <button type="button" className="icon-btn" data-popover-toggle aria-label="Voice settings" aria-expanded={tuning} onClick={() => { setPicker(false); setTuning((t) => !t); }}>
              <Sliders />
            </button>
            {picker && <VoicePicker onClose={() => setPicker(false)} />}
            {tuning && <TuningPopover voice={voice} onClose={() => setTuning(false)} />}
            <div className="grow" />
            {chars > 400 && <span className="count">{chars.toLocaleString()} chars · no limit</span>}
            <span className="hints mono">
              <span>↵ speak</span>
              <span>⇧↵ newline</span>
            </span>
            <button type="button" aria-label="Speak" className={`round iri speak${job ? " busy" : ""}`} disabled={!text.trim() || !!job || !voice?.installed} onClick={submit} style={{ marginLeft: 6 }}>
              <Arrow />
            </button>
          </div>
        </div>
        {showIdeas ? (
          <div className="ideas" role="group" aria-label="Ideas to try">
            <span className="mono">try</span>
            {IDEAS.map((i) => (
              <button key={i.label} type="button" className="idea" onClick={() => fillIdea(i.text)}>
                {i.label}
              </button>
            ))}
          </div>
        ) : (
          <ResultPill />
        )}
      </main>
      <footer className="footer">runs on your machine · no limits · no credits</footer>
    </>
  );
}
