import { call } from "../lib/api";
import { useStore } from "../lib/store";
import type { FixAction, MelError } from "../lib/types";
import { Close } from "./icons";

function fixLabel(fix: FixAction): string | null {
  switch (fix.kind) {
    case "retry":
      return null;
    case "free_space":
      return "see storage";
    case "repair":
      return "repair";
    case "sign_in":
      return "sign in";
    case "open_page":
      return "open page";
    case "get_voice":
    case "install_recipe":
      return "get it";
    case "accept_license":
      return "review terms";
    case "open_settings":
      return "settings";
  }
}

/** Run a fix action. Shared by toasts and inline error rows. */
export async function runFix(fix: FixAction): Promise<void> {
  const s = useStore.getState();
  try {
    switch (fix.kind) {
      case "free_space":
      case "open_settings":
        s.go("settings");
        return;
      case "repair":
        s.notify("repairing — this can take a minute");
        await call((b) => b.repairEngine(fix.family));
        s.notify("repaired — try that again");
        return;
      case "sign_in":
        s.setSignInFor({ recipeId: null, label: "this voice", gateUrl: null, voiceId: null });
        return;
      case "open_page":
        await call((b) => b.openUrl(fix.url));
        return;
      case "get_voice": {
        const v = s.voices.find((x) => x.id === fix.voice_id);
        if (v) await s.getVoice(v);
        return;
      }
      case "install_recipe":
        await call((b) => b.installRecipe(fix.recipe_id));
        return;
      case "accept_license":
      case "retry":
        return;
    }
  } catch (e) {
    s.showError(e as MelError);
  }
}

export function Toasts() {
  const toasts = useStore((s) => s.toasts);
  const dismiss = useStore((s) => s.dismissToast);
  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => {
        const fix = t.kind === "error" ? t.error.fix : null;
        const label = fix ? fixLabel(fix) : null;
        return (
          <div className={`toast ${t.kind}`} key={t.id}>
            <span className="dot" aria-hidden="true" />
            <span className="msg">{t.error.message}</span>
            {label && fix && (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  dismiss(t.id);
                  void runFix(fix);
                }}
              >
                {label}
              </button>
            )}
            {t.action && (
              <button
                type="button"
                className="btn"
                onClick={() => {
                  dismiss(t.id);
                  t.action?.run();
                }}
              >
                {t.action.label}
              </button>
            )}
            <button type="button" className="icon-btn" aria-label="Dismiss" onClick={() => dismiss(t.id)}>
              <Close size={12} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
