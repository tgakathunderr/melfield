import { useEffect, useRef, useState } from "react";

import { drawFillField } from "../canvas/fillField";
import { addDrawer } from "../canvas/loop";
import { Logo } from "../components/icons";
import { audioBus } from "../lib/audioBus";
import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { MelError } from "../lib/types";

type Phase = "downloading" | "ready" | "failed";

export function Onboarding() {
  const ref = useRef<HTMLCanvasElement>(null);
  const app = useStore((s) => s.app);
  const voices = useStore((s) => s.voices);
  const pulls = useStore((s) => s.pulls);
  const [phase, setPhase] = useState<Phase>("downloading");
  const [error, setError] = useState<MelError | null>(null);
  const continueRef = useRef<HTMLButtonElement>(null);

  const starters = voices.filter((v) => v.starter).map((v) => v.name.toLowerCase());
  const pull = Object.values(pulls)[0];
  const progress = phase === "ready" ? 1 : pull ? pull.bytesDone / Math.max(1, pull.bytesTotal) : 0;
  audioBus.fill = progress;

  const start = async () => {
    setPhase("downloading");
    setError(null);
    try {
      await call((b) => b.startSetup());
      await useStore.getState().refreshVoices();
      await useStore.getState().refreshApp();
      setPhase("ready");
    } catch (e) {
      setError(e as MelError);
      setPhase("failed");
    }
  };

  useEffect(() => {
    void start();
  }, []);

  useEffect(() => {
    if (phase === "ready") continueRef.current?.focus();
  }, [phase]);

  useEffect(() => {
    const cv = ref.current;
    if (!cv) return;
    let shown = 0;
    return addDrawer((t) => {
      shown += (audioBus.fill - shown) * 0.06; // ease the level toward real progress
      drawFillField(cv, t, shown);
    });
  }, []);

  const pct = Math.round(progress * 100);
  const finish = () => {
    const s = useStore.getState();
    const first = s.voices.find((v) => v.installed);
    if (first) s.selectVoice(first.id);
    s.go("studio");
  };

  return (
    <div className="onboarding">
      <canvas ref={ref} className="field" aria-hidden="true" />
      <main className="onboarding-main">
        <Logo size={30} />
        <h1 className="display xl">
          studio voices,
          <br />
          on your machine.
        </h1>
        <ol className="steps" aria-live="polite">
          <li>
            <span style={{ color: "var(--green)" }}>✓</span>
            <span>{app?.hardware.summary ?? "checking your computer"}</span>
          </li>
          <li className={phase === "ready" ? "" : "now"}>
            <span style={{ color: phase === "failed" ? "var(--rec-red)" : phase === "ready" ? "var(--green)" : "var(--orange)" }}>
              {phase === "ready" ? "✓" : phase === "failed" ? "!" : "↓"}
            </span>
            <span className="grow">{phase === "failed" && error ? error.message : starters.join(", ") || "starter voices"}</span>
            {phase === "downloading" && <span style={{ color: "var(--text-muted)" }}>{pct}%</span>}
          </li>
          <li className={phase === "ready" ? "now" : "later"}>
            <span style={{ color: phase === "ready" ? "var(--green)" : undefined }}>{phase === "ready" ? "✓" : "○"}</span>
            <span>ready to speak</span>
          </li>
        </ol>
        {phase === "failed" ? (
          <button type="button" className="btn lg iri" onClick={() => void start()}>try again</button>
        ) : (
          <button ref={continueRef} type="button" className="btn lg iri" disabled={phase !== "ready"} onClick={finish}>
            Continue
          </button>
        )}
        <p className="mono">no account · no api keys · no terminal</p>
      </main>
    </div>
  );
}
