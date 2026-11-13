import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

async function onboard(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: /studio voices/i })).toBeVisible();
  const cont = page.getByRole("button", { name: "Continue" });
  await expect(cont).toBeEnabled({ timeout: 10_000 });
  await cont.click();
  await expect(page.getByRole("heading", { name: "what should I say?" })).toBeVisible();
}

async function noSeriousA11yIssues(page: Page) {
  const results = await new AxeBuilder({ page }).disableRules(["region"]).analyze();
  const serious = results.violations.filter((v) => v.impact === "serious" || v.impact === "critical");
  expect(serious.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`)).toEqual([]);
}

test("first launch → hear a voice in studio", async ({ page }) => {
  await onboard(page);
  await noSeriousA11yIssues(page);
  const box = page.getByLabel("Text to speak");
  await box.fill("Hello from Melfield.");
  await box.press("Enter");
  await expect(page.getByRole("button", { name: /play clip|pause/i })).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/aria · 0:0\d/)).toBeVisible();
});

test("shift+enter adds a newline instead of speaking", async ({ page }) => {
  await onboard(page);
  const box = page.getByLabel("Text to speak");
  await box.fill("line one");
  await box.press("Shift+Enter");
  await box.type("line two");
  await expect(box).toHaveValue("line one\nline two");
  await expect(page.getByRole("button", { name: /play clip/i })).toHaveCount(0);
});

test("voices: filter, get a voice, sign-in sheet for gated voices", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Voices", exact: true }).click();
  await expect(page.getByRole("heading", { name: "pick a voice" })).toBeVisible();
  await noSeriousA11yIssues(page);
  await page.getByRole("button", { name: "Hindi" }).click();
  await expect(page.getByText("Ananya")).toBeVisible();
  await expect(page.getByText("Aria")).toHaveCount(0);
  await page.getByRole("button", { name: /^Get/ }).first().click();
  await expect(page.getByRole("dialog", { name: /sign-in/i })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
});

test("clone: create stays locked until consent", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Clone", exact: true }).click();
  await expect(page.getByRole("heading", { name: "lend me your voice" })).toBeVisible();
  await noSeriousA11yIssues(page);
  const create = page.getByRole("button", { name: "Create voice" });
  await expect(create).toBeDisabled();
  await page.getByRole("button", { name: /or choose an audio file/ }).click();
  await page.getByLabel("Voice name").fill("Sam");
  await expect(create).toBeDisabled();
  await page.getByRole("switch").click();
  await expect(create).toBeEnabled();
  await create.click();
  await expect(page.getByRole("heading", { name: "hello, sam." })).toBeVisible();
});

test("settings: everything technical lives under advanced", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await expect(page.getByRole("heading", { name: "settings" })).toBeVisible();
  await noSeriousA11yIssues(page);
  await expect(page.getByText("local api")).toBeHidden();
  await page.getByText("advanced").click();
  await expect(page.getByText("local api")).toBeVisible();
});

test("main screens never show engine names", async ({ page }) => {
  await onboard(page);
  for (const screen of ["Studio", "Voices", "Clone"]) {
    await page.getByRole("button", { name: screen, exact: true }).click();
    const text = (await page.locator("body").innerText()).toLowerCase();
    for (const banned of ["kokoro", "qwen", "chatterbox", "parler", "voxcpm", "onnx", "vram", "cuda", "python"]) {
      expect(text, `${screen} mentions ${banned}`).not.toContain(banned);
    }
  }
});

test("keyboard: history opens and closes, focus stays visible", async ({ page }) => {
  await onboard(page);
  await page.keyboard.press("Control+h");
  await expect(page.getByRole("dialog", { name: "History" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "History" })).toHaveCount(0);
  await page.keyboard.press("Tab");
  const outline = await page.evaluate(() => getComputedStyle(document.activeElement as Element).boxShadow);
  expect(outline).not.toBe("none");
});

async function opened(page: Page): Promise<string[]> {
  return page.evaluate(() => (window as unknown as { __melfieldOpened: string[] }).__melfieldOpened);
}

test("voice picker opens, lists voices and switches voice (regression: crashed the app)", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Aria" }).click();
  const list = page.getByRole("listbox", { name: "Choose a voice" });
  await expect(list).toBeVisible();
  await expect(list.getByRole("option")).toHaveCount(4);
  await page.keyboard.press("ArrowDown");
  await list.getByRole("option", { name: /Mira/ }).click();
  await expect(list).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Mira" })).toBeVisible();
  await expect(page.getByText("that wasn't supposed to happen")).toHaveCount(0);
  await page.getByRole("button", { name: "Mira" }).click();
  await page.getByRole("button", { name: "Hear Theo" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("listbox")).toHaveCount(0);
});

test("try ideas fill the composer; save shows 'show in folder' that reveals the file", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "story opener" }).click();
  await expect(page.getByLabel("Text to speak")).toHaveValue(/lighthouse/);
  await page.getByLabel("Text to speak").press("Enter");
  await page.getByRole("button", { name: "Save clip" }).click({ timeout: 10_000 });
  await expect(page.getByText(/^saved /)).toBeVisible();
  await page.getByRole("button", { name: "show in folder" }).click();
  expect((await opened(page)).some((u) => u.startsWith("reveal:"))).toBe(true);
});

test("sign-in walks through clear steps and every link opens", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Voices", exact: true }).click();
  await page.getByRole("button", { name: "Hindi" }).click();
  await page.getByRole("button", { name: /^Get/ }).first().click();
  const sheet = page.getByRole("dialog", { name: "one free sign-in" });
  await expect(sheet).toBeVisible();
  await sheet.getByRole("button", { name: "create one" }).click();
  await sheet.getByRole("button", { name: "make a key" }).click();
  const urls = await opened(page);
  expect(urls).toContain("https://huggingface.co/join");
  expect(urls.some((u) => u.startsWith("https://huggingface.co/settings/tokens/new"))).toBe(true);
  await sheet.getByLabel("Hugging Face key").fill("hf_abcdefghijklmnopqrstuvwxyz0123");
  await expect(sheet.getByText(/connected as you/)).toBeVisible();
  await sheet.getByRole("button", { name: "open the terms page" }).click();
  expect(await opened(page)).toContain("https://huggingface.co/ai4bharat/indic-parler-tts");
  await expect(sheet).toHaveCount(0, { timeout: 10_000 });
  await expect(page.getByText("ananya is ready")).toBeVisible({ timeout: 10_000 });
});

test("settings links: folder, notices and sign-in all respond", async ({ page }) => {
  await onboard(page);
  await page.getByRole("button", { name: "Settings", exact: true }).click();
  await page.getByRole("button", { name: "open folder" }).click();
  await page.getByText("advanced").click();
  await page.getByRole("button", { name: "view" }).click();
  const urls = await opened(page);
  expect(urls).toContain("folder:data");
  expect(urls).toContain("notices");
  await page.getByRole("button", { name: "sign in" }).click();
  await expect(page.getByRole("dialog", { name: "one free sign-in" })).toBeVisible();
});
