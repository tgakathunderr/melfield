import { useStore, type Screen } from "../lib/store";
import { Gear, HistoryIcon, Logo } from "./icons";

const NAV: { id: Screen; label: string }[] = [
  { id: "studio", label: "Studio" },
  { id: "voices", label: "Voices" },
  { id: "clone", label: "Clone" },
];

export function Header() {
  const screen = useStore((s) => s.screen);
  const go = useStore((s) => s.go);
  const setHistoryOpen = useStore((s) => s.setHistoryOpen);
  return (
    <header className="header" data-tauri-drag-region>
      <button type="button" className="wordmark" onClick={() => go("studio")} aria-label="melfield — go to studio">
        <Logo />
        <span>melfield</span>
      </button>
      <nav aria-label="Main" className="glass-pill">
        {NAV.map((n) => (
          <button key={n.id} type="button" className="seg" aria-current={screen === n.id ? "page" : undefined} onClick={() => go(n.id)}>
            {n.label}
          </button>
        ))}
      </nav>
      <div className="right-cluster">
        <span className="local" title="everything runs on this computer">local</span>
        <button type="button" className="btn" onClick={() => setHistoryOpen(true)}>
          <HistoryIcon />
          History
        </button>
        <button type="button" className="icon-btn outline" aria-label="Settings" aria-current={screen === "settings" ? "page" : undefined} onClick={() => go("settings")}>
          <Gear />
        </button>
      </div>
    </header>
  );
}
