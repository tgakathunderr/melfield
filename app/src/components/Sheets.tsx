import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";

import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { MelError } from "../lib/types";

interface SheetProps {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
}

/** Modal sheet: focus moves in, Esc closes, Tab is trapped, focus returns on close. */
export function Sheet({ title, onClose, children, wide }: SheetProps) {
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const prev = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("[data-autofocus], input, button")?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close.current();
      if (e.key === "Tab" && ref.current) {
        const items = Array.from(ref.current.querySelectorAll<HTMLElement>("button:not(:disabled), input, a[href]"));
        if (items.length === 0) return;
        const [a, z] = [items[0], items[items.length - 1]];
        if (e.shiftKey && document.activeElement === a) {
          e.preventDefault();
          z.focus();
        } else if (!e.shiftKey && document.activeElement === z) {
          e.preventDefault();
          a.focus();
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      prev?.focus?.();
    };
  }, []);
  return (
    <div className="sheet-scrim" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`sheet${wide ? " wide" : ""}`} role="dialog" aria-modal="true" aria-label={title} ref={ref}>
        <h2>{title}</h2>
        {children}
      </div>
    </div>
  );
}

const TOKEN_URL = "https://huggingface.co/settings/tokens/new?tokenType=read&description=melfield";
const JOIN_URL = "https://huggingface.co/join";

type StepState = "todo" | "now" | "done";

function Step({ n, state, title, children }: { n: number; state: StepState; title: string; children?: ReactNode }) {
  return (
    <li className={`step ${state}`} aria-current={state === "now" ? "step" : undefined}>
      <span className="step-n" aria-hidden="true">{state === "done" ? "✓" : n}</span>
      <div className="step-body">
        <b>{title}</b>
        {state === "now" && children}
      </div>
    </li>
  );
}

/**
 * Hugging Face sign-in for gated voices and the cloning engine.
 * Three concrete steps, each with its own button; it notices access on its own.
 */
export function SignInSheet() {
  const target = useStore((s) => s.signInFor);
  const signedIn = useStore((s) => s.app?.hfSignedIn ?? false);
  const [oauth, setOauth] = useState(false);
  const [token, setToken] = useState("");
  const [account, setAccount] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [agreedClicked, setAgreedClicked] = useState(false);

  useEffect(() => {
    void call((b) => b.hfOAuthAvailable()).then(setOauth);
  }, []);
  useEffect(() => {
    setToken("");
    setError(null);
    setAgreedClicked(false);
  }, [target?.recipeId, target?.voiceId]);

  const connected = signedIn || account !== null;
  const needsAgree = !!target?.gateUrl;

  const finish = useCallback(async () => {
    const s = useStore.getState();
    s.setSignInFor(null);
    await s.refreshApp();
    await s.refreshVoices();
    const voice = target?.voiceId ? useStore.getState().voices.find((v) => v.id === target.voiceId) : undefined;
    if (voice) await useStore.getState().getVoice({ ...voice, needsSignIn: false });
    else s.notify("signed in — you're all set");
  }, [target]);

  const checkAccess = useCallback(async (): Promise<boolean> => {
    if (!target?.recipeId) return true;
    try {
      return await call((b) => b.hfCheckAccess(target.recipeId as string));
    } catch {
      return false;
    }
  }, [target]);

  // Once connected (and the terms page has been opened), quietly poll for access.
  useEffect(() => {
    if (!target || !connected) return;
    if (!needsAgree) {
      void finish();
      return;
    }
    if (!agreedClicked) return;
    let alive = true;
    const tick = async () => {
      if (alive && (await checkAccess())) {
        alive = false;
        await finish();
      }
    };
    const t = window.setInterval(() => void tick(), 3000);
    window.addEventListener("focus", tick);
    void tick();
    return () => {
      alive = false;
      window.clearInterval(t);
      window.removeEventListener("focus", tick);
    };
  }, [target, connected, needsAgree, agreedClicked, checkAccess, finish]);

  if (!target) return null;
  const close = () => useStore.getState().setSignInFor(null);
  const open = (url: string) => void call((b) => b.openUrl(url)).catch((e: MelError) => setError(e.message));

  const connect = async (value: string) => {
    const key = value.trim();
    if (!key.startsWith("hf_") || key.length < 20) return;
    setBusy(true);
    setError(null);
    try {
      setAccount(await call((b) => b.hfSignInWithToken(key)));
    } catch (e) {
      setError((e as MelError).message);
    } finally {
      setBusy(false);
    }
  };

  const keyState: StepState = connected ? "done" : "now";
  const agreeState: StepState = !connected ? "todo" : "now";

  return (
    <Sheet title="one free sign-in" onClose={close} wide>
      <p>
        <b>{target.label}</b> comes from hugging face — the free home of open voice models. its makers ask everyone to sign in once
        {needsAgree ? " and agree to their terms" : ""}. after that it downloads and runs on this computer, offline.
      </p>
      <ol className="steps-list">
        {oauth ? (
          <Step n={1} state={keyState} title={connected ? `signed in${account ? ` as ${account}` : ""}` : "sign in with hugging face"}>
            <div className="row-left">
              <button type="button" className="btn iri" data-autofocus onClick={() => void call((b) => b.hfOAuthBegin()).catch((e: MelError) => setError(e.message))}>
                sign in with hugging face
              </button>
              <span className="mono">opens your browser · come back when it says you're signed in</span>
            </div>
          </Step>
        ) : (
          <>
            <Step n={1} state={connected ? "done" : "now"} title="have a free hugging face account">
              <div className="row-left">
                <button type="button" className="btn ghost" onClick={() => open(JOIN_URL)}>create one</button>
                <span className="mono">already have one? skip to step 2</span>
              </div>
            </Step>
            <Step n={2} state={keyState} title={connected ? `connected${account ? ` as ${account}` : ""}` : "make a key and paste it here"}>
              <div className="row-left">
                <button type="button" className="btn" data-autofocus onClick={() => open(TOKEN_URL)}>make a key</button>
                <span className="mono">click "create token", then copy it</span>
              </div>
              <label className="sr-only" htmlFor="hf-token">Hugging Face key</label>
              <div className="paste-row">
                <input
                  id="hf-token"
                  placeholder="paste your key — it starts with hf_"
                  value={token}
                  autoComplete="off"
                  spellCheck={false}
                  onChange={(e) => {
                    setToken(e.target.value);
                    if (e.target.value.trim().length >= 30) void connect(e.target.value);
                  }}
                  onKeyDown={(e) => e.key === "Enter" && void connect(token)}
                />
                <button type="button" className="btn iri" disabled={busy || token.trim().length < 20} onClick={() => void connect(token)}>
                  {busy ? "checking…" : "connect"}
                </button>
              </div>
              <span className="mono">the key stays in this computer's secure keychain and is only used to download.</span>
            </Step>
          </>
        )}
        {needsAgree && (
          <Step n={oauth ? 2 : 3} state={agreeState} title={`agree to ${target.label.toLowerCase()}'s terms`}>
            <div className="row-left">
              <button
                type="button"
                className="btn iri"
                onClick={() => {
                  setAgreedClicked(true);
                  open(target.gateUrl as string);
                }}
              >
                open the terms page
              </button>
              <span className="mono">
                {agreedClicked ? "waiting for you to tap \"agree\" there — this updates on its own" : "tap \"agree and access repository\" on that page"}
              </span>
            </div>
          </Step>
        )}
      </ol>
      {error && (
        <p role="alert" className="error-line">
          {error}
        </p>
      )}
      <div className="row">
        <button type="button" className="btn ghost" onClick={close}>not now</button>
        {connected && agreedClicked && (
          <button type="button" className="btn" disabled={busy} onClick={() => void checkAccess().then((ok) => (ok ? finish() : setError("not yet — tap \"agree\" on the terms page, then come back")))}>
            check again
          </button>
        )}
      </div>
    </Sheet>
  );
}

/** Explicit accept step for personal-use (non-commercial) voices. */
export function LicenseSheet() {
  const voice = useStore((s) => s.licenseFor);
  if (!voice) return null;
  const close = () => useStore.getState().setLicenseFor(null);
  const accept = async () => {
    await call((b) => b.acceptLicense(voice.recipeId));
    close();
    await useStore.getState().refreshVoices();
    const fresh = useStore.getState().voices.find((v) => v.id === voice.id);
    if (fresh) await useStore.getState().getVoice({ ...fresh, license: { ...fresh.license, accepted: true } });
  };
  return (
    <Sheet title="personal use only" onClose={close}>
      <p>
        {voice.name} is free for personal projects, but its makers don't allow commercial use — no ads, client work or paid content.
        License: <span className="code">{voice.license.id}</span>
      </p>
      {voice.license.usagePolicy && <p className="mono md">{voice.license.usagePolicy}</p>}
      <div className="row">
        <button type="button" className="btn ghost" onClick={() => void call((b) => b.openUrl(voice.license.url))}>read the terms</button>
        <button type="button" className="btn lg iri" onClick={() => void accept()}>i understand</button>
      </div>
    </Sheet>
  );
}

interface ConfirmProps {
  title: string;
  body: string;
  confirm: string;
  onConfirm: () => void;
  onClose: () => void;
}

export function ConfirmSheet({ title, body, confirm, onConfirm, onClose }: ConfirmProps) {
  return (
    <Sheet title={title} onClose={onClose}>
      <p>{body}</p>
      <div className="row">
        <button type="button" className="btn ghost" data-autofocus onClick={onClose}>keep it</button>
        <button
          type="button"
          className="btn lg danger"
          onClick={() => {
            onConfirm();
            onClose();
          }}
        >
          {confirm}
        </button>
      </div>
    </Sheet>
  );
}
