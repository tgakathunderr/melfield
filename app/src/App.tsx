import { Component, useEffect, type ReactNode } from "react";

import { Header } from "./components/Header";
import { LicenseSheet, SignInSheet } from "./components/Sheets";
import { Toasts } from "./components/Toasts";
import { useStore } from "./lib/store";
import type { MelError } from "./lib/types";
import { Clone } from "./screens/Clone";
import { HistoryDrawer } from "./screens/HistoryDrawer";
import { Onboarding } from "./screens/Onboarding";
import { Settings } from "./screens/Settings";
import { Studio } from "./screens/Studio";
import { Voices } from "./screens/Voices";

/** Never show a stack trace: render-time crashes become a calm message. */
class Boundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  render() {
    if (this.state.failed) {
      return (
        <main className="onboarding" style={{ gap: 20 }}>
          <h1 className="display lg">that wasn't supposed to happen.</h1>
          <p className="mono md">your voices and history are safe.</p>
          <button type="button" className="btn lg iri" onClick={() => window.location.reload()}>reload melfield</button>
        </main>
      );
    }
    return this.props.children;
  }
}

function Screen() {
  const screen = useStore((s) => s.screen);
  switch (screen) {
    case "voices":
      return <Voices />;
    case "clone":
      return <Clone />;
    case "settings":
      return <Settings />;
    default:
      return <Studio />;
  }
}

export function App() {
  const ready = useStore((s) => s.ready);
  const screen = useStore((s) => s.screen);
  const init = useStore((s) => s.init);

  const platform = useStore((s) => s.app?.hardware.platform ?? "");

  useEffect(() => {
    init().catch((e: MelError) => useStore.getState().showError(e));
  }, [init]);

  // Lets CSS clear the macOS traffic lights under the overlay title bar.
  useEffect(() => {
    document.documentElement.dataset.platform = platform;
  }, [platform]);

  // Ctrl/Cmd + 1/2/3 switch screens; Ctrl/Cmd+H opens history.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      const s = useStore.getState();
      if (s.screen === "onboarding") return;
      const map: Record<string, "studio" | "voices" | "clone"> = { "1": "studio", "2": "voices", "3": "clone" };
      if (map[e.key]) {
        e.preventDefault();
        s.go(map[e.key]);
      } else if (e.key.toLowerCase() === "h") {
        e.preventDefault();
        s.setHistoryOpen(!s.historyOpen);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <Boundary>
      <div className="app">
        {!ready ? null : screen === "onboarding" ? (
          <Onboarding />
        ) : (
          <>
            <Header />
            <Screen />
            <HistoryDrawer />
          </>
        )}
        <SignInSheet />
        <LicenseSheet />
        <Toasts />
      </div>
    </Boundary>
  );
}
