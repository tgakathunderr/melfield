import { useMemo, useRef, useState } from "react";

import { OrbCanvas } from "../components/OrbCanvas";
import { ConfirmSheet } from "../components/Sheets";
import { Download, Play, Search, Trash } from "../components/icons";
import { audioBus } from "../lib/audioBus";
import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { MelError, Voice } from "../lib/types";

const FILTERS = ["All", "English", "Hindi", "Multilingual", "My clones"] as const;
type Filter = (typeof FILTERS)[number];

function hashSeed(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) | 0;
  return Math.abs(h) % 97;
}

function VoiceCard({ voice, onRemove }: { voice: Voice; onRemove: (v: Voice) => void }) {
  const pull = useStore((s) => s.pulls[voice.recipeId]);
  const env = useStore((s) => s.envs[voice.recipeId.split("-")[0]]);
  const player = useStore((s) => s.player);
  const hoverTimer = useRef<number | undefined>(undefined);
  const previewId = `preview:${voice.id}`;
  const previewing = player.id === previewId && player.playing;
  const previewingRef = useRef(false);
  previewingRef.current = previewing;

  const preview = () => void call((b) => b.previewVoice(voice.id)).catch((e: MelError) => useStore.getState().showError(e));
  // Hover-to-hear: a short dwell avoids firing while the pointer just passes over.
  const onEnter = () => {
    if (!voice.hasSample && !voice.installed) return;
    hoverTimer.current = window.setTimeout(preview, 220);
  };
  const onLeave = () => window.clearTimeout(hoverTimer.current);

  const use = () => {
    const s = useStore.getState();
    s.selectVoice(voice.id);
    s.go("studio");
  };

  const downloading = voice.installing || !!pull;
  const pct = pull ? Math.round((pull.bytesDone / Math.max(1, pull.bytesTotal)) * 100) : 0;

  return (
    <article className="card" onMouseEnter={onEnter} onMouseLeave={onLeave} aria-label={`${voice.name}, ${voice.meta}`}>
      {voice.license.personalUse && <span className="badge">personal use</span>}
      {voice.kind === "clone" && <span className="badge clone">yours</span>}
      <OrbCanvas orb={voice.orb} seed={hashSeed(voice.id)} pulse={() => (previewingRef.current ? 0.35 + audioBus.energy : 0)} />
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 4 }}>
        <div className="vname">{voice.name}</div>
        <div className="mono">{voice.meta}</div>
        {voice.slowHere && !voice.installed && <div className="mono slow">slow on this computer</div>}
      </div>
      <div className="actions">
        {downloading ? (
          <div className="progress-ring" aria-live="polite">
            {env && pct >= 100 ? env.detail : `getting ${voice.name.toLowerCase()} · ${pct}%`}
            <div className="bar"><i style={{ width: `${pct}%` }} /></div>
          </div>
        ) : (
          <>
            <button type="button" aria-label={`Preview ${voice.name}`} className="round iri" style={{ width: 40, height: 40 }} disabled={!voice.hasSample && !voice.installed} onClick={preview} title={!voice.hasSample && !voice.installed ? `get ${voice.name.toLowerCase()} to hear it` : undefined}>
              <Play />
            </button>
            {voice.installed ? (
              <>
                <button type="button" className="btn solid" style={{ minHeight: 40, padding: "0 18px" }} onClick={use}>
                  Use
                </button>
                <button type="button" className="icon-btn" aria-label={`Remove ${voice.name}`} onClick={() => onRemove(voice)}>
                  <Trash />
                </button>
              </>
            ) : (
              <button type="button" className="btn ghost" style={{ minHeight: 40 }} onClick={() => void useStore.getState().getVoice(voice)} title={voice.downloadLabel}>
                <Download size={13} />
                Get <span className="mono" style={{ marginLeft: 2 }}>{voice.downloadLabel}</span>
              </button>
            )}
          </>
        )}
      </div>
    </article>
  );
}

export function Voices() {
  const voices = useStore((s) => s.voices);
  const [filter, setFilter] = useState<Filter>("All");
  const [query, setQuery] = useState("");
  const [removing, setRemoving] = useState<{ voice: Voice; also: string[] } | null>(null);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return voices
      .filter((v) => filter === "All" || v.tags.includes(filter))
      .filter((v) => !q || v.name.toLowerCase().includes(q) || v.meta.toLowerCase().includes(q))
      .sort((a, b) => Number(b.kind === "clone") - Number(a.kind === "clone") || Number(b.installed) - Number(a.installed));
  }, [voices, filter, query]);

  const askRemove = async (v: Voice) => {
    const also = v.kind === "clone" ? [] : await call((b) => b.voicesSharingFiles(v.id));
    setRemoving({ voice: v, also });
  };

  const removeBody = (r: { voice: Voice; also: string[] }) =>
    r.voice.kind === "clone"
      ? `your clone "${r.voice.name}" will be deleted from this computer. this can't be undone.`
      : r.also.length
        ? `${r.voice.name} shares its download with ${r.also.join(", ")} — they'll go too. you can get them back any time.`
        : `${r.voice.name} will be removed to free up space. you can get it back any time.`;

  return (
    <div className="page app-bg-dots">
      <main className="voices-main">
        <h1 className="display lg">pick a voice</h1>
        <div className="toolbar">
          <div role="group" aria-label="Filter voices" className="glass-pill" style={{ flexWrap: "wrap" }}>
            {FILTERS.map((f) => (
              <button key={f} type="button" className="seg" aria-pressed={filter === f} onClick={() => setFilter(f)}>
                {f}
              </button>
            ))}
          </div>
          <div className="search">
            <Search />
            <label htmlFor="voice-search" className="sr-only">Search voices</label>
            <input id="voice-search" type="search" placeholder="search" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
        </div>
        {shown.length === 0 ? (
          <div className="empty">
            <p className="mono md">{filter === "My clones" ? "no clones yet — lend us your voice on the clone tab" : "nothing matches that"}</p>
            {filter === "My clones" && (
              <button type="button" className="btn" onClick={() => useStore.getState().go("clone")}>clone a voice</button>
            )}
          </div>
        ) : (
          <div className="grid">
            {shown.map((v) => (
              <VoiceCard key={v.id} voice={v} onRemove={(x) => void askRemove(x)} />
            ))}
          </div>
        )}
      </main>
      {removing && (
        <ConfirmSheet
          title={removing.voice.kind === "clone" ? "delete this clone?" : `remove ${removing.voice.name.toLowerCase()}?`}
          body={removeBody(removing)}
          confirm={removing.voice.kind === "clone" ? "delete" : "remove"}
          onClose={() => setRemoving(null)}
          onConfirm={() => void call((b) => b.removeVoice(removing.voice.id)).catch((e: MelError) => useStore.getState().showError(e))}
        />
      )}
    </div>
  );
}
