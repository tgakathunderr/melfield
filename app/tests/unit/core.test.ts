import { describe, expect, it } from "vitest";

import { bandAt } from "../../src/canvas/dotField";
import { formatTime } from "../../src/components/DotWave";
import { toMelError } from "../../src/lib/api";
import { audioBus, pushBands } from "../../src/lib/audioBus";
import { createMockBackend } from "../../src/lib/mockBackend";
import type { CoreEvent } from "../../src/lib/types";

describe("audio bus", () => {
  it("tracks energy from the mid bands and decays to zero when stopped", () => {
    const loud = Array.from({ length: 64 }, () => 0.9);
    for (let i = 0; i < 20; i++) pushBands(loud, true);
    expect(audioBus.energy).toBeGreaterThan(0.8);
    for (let i = 0; i < 40; i++) pushBands(loud, false);
    expect(audioBus.energy).toBeLessThan(0.01);
  });
});

describe("dot field", () => {
  it("mirrors bands from the centre outwards", () => {
    const bands = Array.from({ length: 64 }, (_, i) => i / 63);
    expect(bandAt(0.5, bands)).toBe(0);
    expect(bandAt(0, bands)).toBe(1);
    expect(bandAt(1, bands)).toBe(1);
    expect(bandAt(0.25, bands)).toBeCloseTo(bandAt(0.75, bands), 5);
  });
});

describe("formatting", () => {
  it("formats durations like the design (aria · 0:12)", () => {
    expect(formatTime(12.4)).toBe("0:12");
    expect(formatTime(75)).toBe("1:15");
    expect(formatTime(-3)).toBe("0:00");
  });
});

describe("errors", () => {
  it("never surfaces raw exceptions", () => {
    const e = toMelError(new TypeError("Cannot read properties of undefined (reading 'x') at foo.ts:12"));
    expect(e.message).toBe("something went sideways — try that again");
    expect(e.message).not.toContain("foo.ts");
    const mel = { code: "disk_full", message: "free up 1.2 GB", fix: null };
    expect(toMelError(mel)).toBe(mel);
  });
});

describe("mock backend contract", () => {
  it("installs starters, speaks, and records history", async () => {
    const b = createMockBackend();
    const events: CoreEvent[] = [];
    await b.onEvent((e) => events.push(e));
    expect((await b.appState()).firstRun).toBe(true);
    await expect(b.speak({ text: "hi", voiceId: "aria", speed: 1, delivery: "natural", play: true })).rejects.toMatchObject({ code: "voice_not_installed" });
    await b.startSetup();
    expect((await b.voices()).find((v) => v.id === "aria")?.installed).toBe(true);
    expect(events.some((e) => e.type === "pull" && e.payload.state === "installed")).toBe(true);
    await expect(b.speak({ text: "  ", voiceId: "aria", speed: 1, delivery: "natural", play: true })).rejects.toMatchObject({ code: "empty_text" });
  });

  it("refuses clones without consent", async () => {
    const b = createMockBackend();
    const take = await b.importAudio();
    await expect(b.createClone({ name: "Me", recordingId: take!.id, language: "en", refText: null, consent: false })).rejects.toMatchObject({ code: "consent_required" });
    const v = await b.createClone({ name: "Me", recordingId: take!.id, language: "en", refText: null, consent: true });
    expect(v.kind).toBe("clone");
    expect(v.tags).toContain("My clones");
  });

  it("gated voices ask for sign-in and validate keys", async () => {
    const b = createMockBackend();
    await expect(b.getVoice("ananya")).rejects.toMatchObject({ code: "needs_sign_in", fix: { kind: "sign_in" } });
    await expect(b.hfSignInWithToken("nope")).rejects.toMatchObject({ code: "needs_sign_in" });
    await b.hfSignInWithToken("hf_abcdefghijklmnop");
    expect((await b.voices()).find((v) => v.id === "ananya")?.needsSignIn).toBe(false);
  });
});
