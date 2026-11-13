import { call } from "./api";
import { useStore } from "./store";
import type { MelError } from "./types";

/** Save a clip where the person chooses, then offer "show in folder". */
export async function saveGeneration(id: string): Promise<void> {
  const s = useStore.getState();
  try {
    const fmt = s.app?.settings.outputFormat ?? "wav";
    const path = await call((b) => b.exportGeneration(id, fmt));
    if (path) {
      const name = path.split(/[\/]/).pop() ?? "clip";
      s.notify(`saved ${name}`, {
        label: "show in folder",
        run: () => void call((b) => b.revealPath(path)).catch((e: MelError) => useStore.getState().showError(e)),
      });
    }
  } catch (e) {
    s.showError(e as MelError);
  }
}
