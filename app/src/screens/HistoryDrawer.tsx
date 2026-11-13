import { useEffect, useRef, useState } from "react";

import { formatTime } from "../components/DotWave";
import { Close, Download, Pause, Play, Reopen, Search, Star, Trash } from "../components/icons";
import { saveGeneration } from "../lib/actions";
import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { Generation, MelError } from "../lib/types";

const DRAFT_KEY = "melfield.draft";

function when(iso: string): string {
  const d = new Date(iso);
  const mins = Math.round((Date.now() - d.getTime()) / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  if (mins < 60 * 24) return `${Math.round(mins / 60)}h ago`;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" }).toLowerCase();
}

export function HistoryDrawer() {
  const open = useStore((s) => s.historyOpen);
  const setOpen = useStore((s) => s.setHistoryOpen);
  const player = useStore((s) => s.player);
  const lastGeneration = useStore((s) => s.lastGeneration);
  const [items, setItems] = useState<Generation[]>([]);
  const [query, setQuery] = useState("");
  const panel = useRef<HTMLDivElement>(null);

  const load = (q: string) => void call((b) => b.history(q || null, 100, 0)).then(setItems).catch((e: MelError) => useStore.getState().showError(e));

  useEffect(() => {
    if (open) load(query);
  }, [open, query, lastGeneration?.id]);

  useEffect(() => {
    if (!open) return;
    const prev = document.activeElement as HTMLElement | null;
    panel.current?.querySelector<HTMLElement>("input")?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      prev?.focus?.();
    };
  }, [open, setOpen]);

  if (!open) return null;

  const reopen = (g: Generation) => {
    try {
      localStorage.setItem(DRAFT_KEY, g.text);
    } catch {
      /* ignore */
    }
    const s = useStore.getState();
    if (s.voices.some((v) => v.id === g.voiceId && v.installed)) s.selectVoice(g.voiceId);
    s.go("studio");
    // Studio reads the draft on mount; force a remount by toggling screen.
    window.dispatchEvent(new CustomEvent("melfield:reopen", { detail: g.text }));
  };

  const act = (fn: () => Promise<unknown>) => void fn().catch((e: MelError) => useStore.getState().showError(e));

  return (
    <>
      <div className="drawer-scrim" onClick={() => setOpen(false)} />
      <aside className="drawer" role="dialog" aria-modal="true" aria-label="History" ref={panel}>
        <header>
          <h2>history</h2>
          <button type="button" className="icon-btn" aria-label="Close history" onClick={() => setOpen(false)}>
            <Close />
          </button>
        </header>
        <div style={{ padding: "0 18px 6px" }}>
          <div className="search" style={{ width: "100%" }}>
            <Search />
            <label htmlFor="history-search" className="sr-only">Search history</label>
            <input id="history-search" type="search" placeholder="search what you've made" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
        </div>
        <div className="list">
          {items.length === 0 && <p className="mono md empty">{query ? "nothing matches that" : "everything you make shows up here"}</p>}
          {items.map((g) => {
            const playing = player.id === g.id && player.playing;
            return (
              <div className="hist" key={g.id}>
                <button
                  type="button"
                  className="play-btn"
                  aria-label={playing ? `Pause ${g.voiceName}` : `Play ${g.voiceName}`}
                  style={{ flexShrink: 0, width: 32, height: 32 }}
                  onClick={() => act(() => call((b) => (playing ? b.playerControl("pause") : b.playGeneration(g.id))))}
                >
                  {playing ? <Pause size={10} /> : <Play size={10} />}
                </button>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="text">{g.text}</div>
                  <div className="row">
                    <span className="mono">{g.voiceName.toLowerCase()} · {formatTime(g.durationS)} · {when(g.createdAt)}{g.watermarked ? " · watermarked" : ""}</span>
                    <span style={{ flex: 1 }} />
                    <button type="button" className="icon-btn" aria-label="Favourite" aria-pressed={g.favourite} onClick={() => act(async () => { await call((b) => b.setFavourite(g.id, !g.favourite)); load(query); })} style={{ color: g.favourite ? "#FFD27A" : undefined }}>
                      <Star filled={g.favourite} />
                    </button>
                    <button type="button" className="icon-btn" aria-label="Open in studio" onClick={() => reopen(g)}>
                      <Reopen />
                    </button>
                    <button type="button" className="icon-btn" aria-label="Download" onClick={() => void saveGeneration(g.id)}>
                      <Download />
                    </button>
                    <button type="button" className="icon-btn" aria-label="Delete" onClick={() => act(async () => { await call((b) => b.deleteGeneration(g.id)); load(query); })}>
                      <Trash />
                    </button>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      </aside>
    </>
  );
}
