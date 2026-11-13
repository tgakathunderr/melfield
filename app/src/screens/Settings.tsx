import { useEffect, useState, type ReactNode } from "react";

import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { ApiStatus, Compute, DiskUsage, EngineStatus, MelError } from "../lib/types";

function bytes(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${Math.max(1, Math.round(n / 1e6))} MB`;
  return `${Math.max(0, Math.round(n / 1e3))} KB`;
}

function Row({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <div className="setting">
      <div className="label">
        <b>{label}</b>
        {hint && <small>{hint}</small>}
      </div>
      {children}
    </div>
  );
}

function Toggle({ on, label, onChange }: { on: boolean; label: string; onChange: (v: boolean) => void }) {
  return (
    <button type="button" role="switch" className="switch" aria-checked={on} aria-label={label} onClick={() => onChange(!on)}>
      <span className="knob" />
    </button>
  );
}

const guard = (fn: () => Promise<unknown>) => void fn().catch((e: MelError) => useStore.getState().showError(e));

export function Settings() {
  const app = useStore((s) => s.app);
  const voices = useStore((s) => s.voices);
  const update = useStore((s) => s.updateSettings);
  const envs = useStore((s) => s.envs);
  const [usage, setUsage] = useState<DiskUsage | null>(null);
  const [engines, setEngines] = useState<EngineStatus[]>([]);
  const [api, setApi] = useState<ApiStatus | null>(null);
  const [update_, setUpdate] = useState<string | null | undefined>(undefined);
  const [moved, setMoved] = useState(false);

  const refresh = () => {
    guard(() => call((b) => b.diskUsage()).then(setUsage));
    guard(() => call((b) => b.engines()).then(setEngines));
    guard(() => call((b) => b.apiStatus()).then(setApi));
  };
  useEffect(refresh, [voices]);

  if (!app) return null;
  const s = app.settings;
  const installed = voices.filter((v) => v.installed);
  const total = usage ? usage.voicesBytes + usage.enginesBytes + usage.historyBytes + usage.clonesBytes : 0;
  const pctOf = (n: number) => `${total ? (n / total) * 100 : 0}%`;
  const gpu = app.hardware.gpus[0];

  return (
    <div className="page app-bg-dots">
      <main className="settings-main">
        <h1 className="display lg">settings</h1>

        <section className="section" aria-labelledby="s-voice">
          <h2 id="s-voice">voice</h2>
          <Row label="default voice" hint="used when melfield opens">
            <select className="lang-select" aria-label="Default voice" value={s.defaultVoice} onChange={(e) => void update({ defaultVoice: e.target.value })}>
              {installed.map((v) => <option key={v.id} value={v.id}>{v.name.toLowerCase()}</option>)}
            </select>
          </Row>
          <Row label="save format" hint="what the download button saves">
            <div className="glass-pill" role="group" aria-label="Save format">
              {(["wav", "flac"] as const).map((f) => (
                <button key={f} type="button" className="seg" aria-pressed={s.outputFormat === f} onClick={() => void update({ outputFormat: f })}>{f}</button>
              ))}
            </div>
          </Row>
          <Row label="keep history" hint="everything you make stays on this computer">
            <Toggle on={s.keepHistory} label="Keep history" onChange={(v) => void update({ keepHistory: v })} />
          </Row>
        </section>

        <section className="section" aria-labelledby="s-computer">
          <h2 id="s-computer">this computer</h2>
          <Row label="use" hint={gpu ? `${gpu.name.toLowerCase()} · ${app.hardware.summary}` : app.hardware.summary}>
            <div className="glass-pill" role="group" aria-label="Compute">
              {(["auto", "gpu", "cpu"] as Compute[]).filter((c) => c !== "gpu" || app.hardware.gpus.length > 0).map((c) => (
                <button key={c} type="button" className="seg" aria-pressed={s.compute === c} onClick={() => void update({ compute: c })}>
                  {c === "auto" ? "automatic" : c === "gpu" ? "graphics card" : "processor"}
                </button>
              ))}
            </div>
          </Row>
          <Row label="calmer visuals" hint="fewer moving dots — easier on older computers">
            <Toggle on={s.reducedEffects ?? app.hardware.platform.startsWith("linux")} label="Calmer visuals" onChange={(v) => void update({ reducedEffects: v })} />
          </Row>
        </section>

        <section className="section" aria-labelledby="s-storage">
          <h2 id="s-storage">storage</h2>
          {usage && (
            <>
              <div className="usage" aria-hidden="true">
                <i style={{ width: pctOf(usage.voicesBytes) }} />
                <i style={{ width: pctOf(usage.enginesBytes) }} />
                <i style={{ width: pctOf(usage.historyBytes) }} />
                <i style={{ width: pctOf(usage.clonesBytes) }} />
              </div>
              <Row label={`${usage.totalLabel} used · ${bytes(usage.freeBytes)} free`} hint={`voices ${bytes(usage.voicesBytes)} · engines ${bytes(usage.enginesBytes)} · history ${bytes(usage.historyBytes)} · clones ${bytes(usage.clonesBytes)}`}>
                <button type="button" className="btn" onClick={() => guard(() => call((b) => b.openDataDir()))}>open folder</button>
              </Row>
              <Row label="location" hint={moved ? "moved — restart melfield to finish" : usage.location}>
                <button type="button" className="btn" onClick={() => guard(async () => setMoved(await call((b) => b.moveStorage())))}>move…</button>
              </Row>
            </>
          )}
        </section>

        <section className="section" aria-labelledby="s-updates">
          <h2 id="s-updates">updates</h2>
          <Row label={`melfield ${app.appVersion}`} hint={update_ === undefined ? "updates are signed and optional" : update_ ? `version ${update_} is ready` : "you're up to date"}>
            {update_ ? (
              <button type="button" className="btn iri" onClick={() => guard(() => call((b) => b.installUpdate()))}>update & restart</button>
            ) : (
              <button type="button" className="btn" onClick={() => guard(async () => setUpdate(await call((b) => b.checkForUpdate())))}>check now</button>
            )}
          </Row>
          <Row label="check automatically">
            <Toggle on={s.checkUpdates} label="Check for updates automatically" onChange={(v) => void update({ checkUpdates: v })} />
          </Row>
        </section>

        <section className="section" aria-labelledby="s-help">
          <h2 id="s-help">help</h2>
          <Row label="something not working?" hint="export logs to share — they contain no text you typed beyond errors, and no keys">
            <button type="button" className="btn" onClick={() => guard(() => call((b) => b.exportLogs()))}>export logs</button>
          </Row>
          <Row label="hugging face" hint={app.hfSignedIn ? "signed in — gated voices can download" : "only needed for a few voices"}>
            {app.hfSignedIn ? (
              <button type="button" className="btn ghost" onClick={() => guard(async () => { await call((b) => b.hfSignOut()); await useStore.getState().refreshApp(); })}>sign out</button>
            ) : (
              <button type="button" className="btn ghost" onClick={() => useStore.getState().setSignInFor({ recipeId: null, label: "hugging face voices", gateUrl: null, voiceId: null })}>sign in</button>
            )}
          </Row>
        </section>

        <details className="section advanced">
          <summary>advanced</summary>
          <Row label="local api" hint={api?.running ? `openai-compatible · POST ${api.url}/v1/audio/speech` : "let other apps on this computer use melfield's voices (off by default)"}>
            <Toggle on={!!api?.running} label="Local API" onChange={(v) => guard(async () => { setApi(await call((b) => b.setApiEnabled(v))); await useStore.getState().refreshApp(); })} />
          </Row>
          {api?.running && api.token && (
            <Row label="api key" hint="send as: Authorization: Bearer <key>">
              <span className="code">{api.token}</span>
            </Row>
          )}
          <Row label="require a key" hint="other apps must send the api key">
            <Toggle on={s.apiRequireToken} label="Require API key" onChange={(v) => void update({ apiRequireToken: v }).then(refresh)} />
          </Row>
          {engines.map((e) => (
            <Row key={e.family} label={e.label} hint={`${e.recipes.join(" · ")} · ${envs[e.family]?.detail ?? (e.ready ? (e.running ? "running" : "ready") : "not set up")}`}>
              <button type="button" className="btn ghost" disabled={!e.ready && e.family === "native"} onClick={() => guard(async () => { await call((b) => b.repairEngine(e.family)); refresh(); })}>repair</button>
            </Row>
          ))}
          <Row label="model files" hint="every voice comes from its original source, checked and stored here">
            <button type="button" className="btn ghost" onClick={() => guard(() => call((b) => b.openDataDir()))}>show files</button>
          </Row>
          <Row label="licenses & credits" hint="melfield is MIT · voices keep their own licenses">
            <button type="button" className="btn ghost" onClick={() => guard(() => call((b) => b.openNotices()))}>view</button>
          </Row>
          {installed.map((v) => v.license.attribution).filter((a, i, all) => a && all.indexOf(a) === i).map((a) => (
            <p key={a} className="mono" style={{ padding: "0 20px", margin: "4px 0 10px" }}>{a}</p>
          ))}
          <p className="mono" style={{ padding: "4px 20px 14px", margin: 0 }}>data folder · {app.dataDir}</p>
        </details>
      </main>
    </div>
  );
}
