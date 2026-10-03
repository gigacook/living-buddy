import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { mkdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SHOTS = process.env.SCREENSHOT_DIR ?? fileURLToPath(new URL("../test-results/screens", import.meta.url));
mkdirSync(SHOTS, { recursive: true });

async function onboard(page: Page, name: string) {
  await page.goto("/");
  await page.getByLabel("Your name").fill(name);
  await page.getByRole("button", { name: "Let's go" }).click();
  await expect(page.getByRole("heading", { level: 1 })).toContainText(name);
}

async function axe(page: Page, label: string) {
  const results = await new AxeBuilder({ page: page as unknown as ConstructorParameters<typeof AxeBuilder>[0]["page"] }).withTags(["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "wcag22aa"]).analyze();
  const summary = results.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).slice(0, 3).join(" | ")}`);
  expect(summary, `axe violations on ${label}`).toEqual([]);
}

/** True if the page itself can be scrolled horizontally (wide regions should scroll internally). */
async function pageScrollsSideways(page: Page): Promise<boolean> {
  return page.evaluate(() => {
    window.scrollTo(400, window.scrollY);
    const moved = window.scrollX > 0;
    window.scrollTo(0, window.scrollY);
    return moved;
  });
}

async function shot(page: Page, name: string, project: string) {
  await page.screenshot({ path: join(SHOTS, `${project}-${name}.png`), fullPage: true });
}

test("onboarding, quick add, complete and undo", async ({ page }, info) => {
  await onboard(page, `Ada ${info.project.name}`);
  await axe(page, "today (empty)");
  await page.getByLabel("Add something for today").fill("Water the plants");
  await page.getByRole("button", { name: "Add", exact: true }).click();
  await expect(page.locator(".next-title")).toHaveText("Water the plants");
  await shot(page, "today", info.project.name);
  await page.getByRole("button", { name: "Done" }).first().click();
  await expect(page.getByRole("status").filter({ hasText: "Done: “Water the plants”" })).toBeVisible();
  await page.getByRole("button", { name: "Undo" }).click();
  await expect(page.locator(".next-title")).toHaveText("Water the plants");
});

test("household routine from a template rotates and moves to its next date", async ({ page }, info) => {
  await onboard(page, `Ben ${info.project.name}`);
  await page.goto("/groups");
  await page.getByRole("button", { name: "New group" }).first().click();
  await page.getByLabel("Name", { exact: true }).fill(`Flat ${info.project.name}`);
  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByRole("link", { name: new RegExp(`Flat ${info.project.name}`) }).click();
  await page.getByRole("button", { name: "Add routine" }).click();
  await page.getByRole("button", { name: /Dishes/ }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Add routine" }).click();
  await expect(page.getByRole("button", { name: "Mark “Dishes” as done" })).toBeVisible();
  await axe(page, "group household");
  await page.getByRole("button", { name: "Mark “Dishes” as done" }).click();
  await expect(page.getByRole("status").filter({ hasText: /next due tomorrow/i })).toBeVisible();
  await page.getByRole("button", { name: /^Dishes/ }).click();
  await page.getByRole("button", { name: "History" }).click();
  await expect(page.getByText(/finished “Dishes”/)).toBeVisible();
  await page.keyboard.press("Escape");
});

test("project board: add a card and move it with the keyboard-friendly menu", async ({ page }, info) => {
  await onboard(page, `Cleo ${info.project.name}`);
  await page.goto("/groups");
  await page.getByRole("button", { name: "New group" }).first().click();
  await page.getByLabel("Name", { exact: true }).fill(`Garden ${info.project.name}`);
  await page.getByLabel(/Project — a goal/).check();
  await page.getByRole("textbox", { name: "Goal" }).fill("Herbs by spring");
  await page.getByRole("button", { name: "Create group" }).click();
  await page.getByRole("link", { name: new RegExp(`Garden ${info.project.name}`) }).click();
  await page.getByRole("button", { name: "Add task" }).click();
  await page.getByLabel("What needs doing?").fill("Buy soil");
  await page.getByLabel("Due date").fill(new Date(Date.now() + 3 * 86400000).toISOString().slice(0, 10));
  await page.getByRole("button", { name: "Add task" }).last().click();
  const card = page.locator("article", { hasText: "Buy soil" });
  await expect(card).toBeVisible();
  await card.getByLabel("Move “Buy soil” to column").selectOption("in_progress");
  await expect(page.locator("section[aria-label^='In progress']").getByRole("button", { name: "Buy soil", exact: true })).toBeVisible();
  await card.getByLabel("Move “Buy soil” to column").selectOption("done");
  await expect(page.locator("section[aria-label^='Done']").getByRole("button", { name: "Buy soil", exact: true })).toBeVisible();
  await axe(page, "project board");
  expect(await pageScrollsSideways(page)).toBe(false);
  await shot(page, "board", info.project.name);
  await page.getByRole("button", { name: "Timeline" }).click();
  await expect(page.getByRole("region", { name: "Project timeline" })).toBeVisible();
});

test("focus timer keeps running across a reload and can pause", async ({ page }, info) => {
  await onboard(page, `Dev ${info.project.name}`);
  await page.goto("/focus");
  await page.getByRole("button", { name: "15 min" }).click();
  await page.getByRole("button", { name: "Start" }).click();
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await axe(page, "focus running");
  await shot(page, "focus", info.project.name);
  await page.waitForTimeout(2200);
  await page.reload();
  const digits = page.getByRole("timer");
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  const text = await digits.textContent();
  expect(text).toMatch(/^14:5\d$/);
  await page.getByRole("button", { name: "Pause" }).click();
  await expect(page.getByRole("button", { name: "Resume" })).toBeVisible();
  await page.getByRole("button", { name: "Reset" }).click();
  await expect(page.getByRole("button", { name: "Start" })).toBeVisible();
});

test("calendar import, agenda, export and a revocable share link", async ({ page, context }, info) => {
  await onboard(page, `Eve ${info.project.name}`);
  const d = new Date(Date.now() + 2 * 86400000);
  const ymd = `${d.getFullYear()}${String(d.getMonth() + 1).padStart(2, "0")}${String(d.getDate()).padStart(2, "0")}`;
  const ics = `BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\nUID:e2e-${info.project.name}@test\r\nDTSTART:${ymd}T150000Z\r\nDTEND:${ymd}T160000Z\r\nSUMMARY:Piano lesson\r\nCATEGORIES:School\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n`;
  await page.goto("/calendar");
  await page.getByRole("button", { name: "Calendars" }).click();
  await page.getByRole("button", { name: "Import file" }).click();
  await page.getByLabel("Calendar file (.ics)").setInputFiles({ name: "school.ics", mimeType: "text/calendar", buffer: Buffer.from(ics) });
  await page.getByRole("button", { name: "Import", exact: true }).click();
  await expect(page.getByRole("status").filter({ hasText: /1 new/ })).toBeVisible();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Agenda" }).click();
  await expect(page.getByRole("button", { name: /Piano lesson/ })).toBeVisible();
  await axe(page, "calendar agenda");
  await page.getByRole("button", { name: "Month" }).click();
  await shot(page, "calendar", info.project.name);

  const download = page.waitForEvent("download");
  await page.locator("summary", { hasText: "Export" }).click();
  await page.getByRole("button", { name: "Download .ics" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("tendly.ics");

  // Enable sharing as the local administrator, then share the imported calendar.
  await page.goto("/settings");
  await page.getByLabel(/Allow read-only share links/).check();
  await page.goto("/calendar");
  await page.getByRole("button", { name: "Share" }).click();
  await page.getByLabel("Name for this link").fill("For grandma");
  await page.getByRole("button", { name: /school/ }).click();
  await page.getByRole("button", { name: "Create link" }).click();
  const url = await page.locator("code", { hasText: "/share/" }).first().textContent();
  expect(url).toBeTruthy();
  const viewer = await context.newPage();
  await viewer.goto(url!);
  await expect(viewer.getByRole("heading", { name: "For grandma" })).toBeVisible();
  await expect(viewer.getByText("Piano lesson")).toBeVisible();
  await page.getByRole("button", { name: "Done" }).click();
  await page.getByRole("button", { name: "Revoke" }).first().click();
  const res = await viewer.goto(url!);
  expect(res?.status()).toBe(404);
});

test("inbox: paste a message and confirm a suggestion", async ({ page }, info) => {
  await onboard(page, `Finn ${info.project.name}`);
  await page.goto("/inbox");
  await page.locator("summary", { hasText: "Paste a message" }).click();
  await page.getByLabel("Subject (optional)").fill("Library notice");
  await page.getByLabel("Message text").fill("Please return your books. They are due 2030-05-20.");
  await page.getByRole("button", { name: "Find tasks and dates" }).click();
  await expect(page.getByText("2030-05-20")).toBeVisible();
  await axe(page, "inbox");
  await page.getByRole("button", { name: "Add as task" }).click();
  await page.getByRole("button", { name: "Confirm and add" }).click();
  await page.getByRole("button", { name: "Added" }).click();
  await expect(page.getByText("Library notice", { exact: true })).toBeVisible();
  await page.goto("/tasks");
  await expect(page.getByRole("button", { name: "Mark “Library notice” as done" })).toBeVisible();
});

test("every main page passes automated accessibility checks", async ({ page }, info) => {
  await onboard(page, `Gus ${info.project.name}`);
  for (const path of ["/", "/tasks", "/focus", "/calendar", "/groups", "/inbox", "/settings", "/notifications"]) {
    await page.goto(path);
    await page.waitForLoadState("networkidle");
    await axe(page, path);
    expect(await pageScrollsSideways(page), `horizontal page scroll on ${path}`).toBe(false);
  }
  await page.goto("/settings");
  await shot(page, "settings", info.project.name);
  // Dark theme also passes.
  await page.getByRole("button", { name: "Dark" }).click();
  await page.goto("/");
  await axe(page, "today dark");
  await shot(page, "today-dark", info.project.name);
});

test("keyboard users can reach the main content and dialogs trap focus", async ({ page }, info) => {
  test.skip(info.project.name.startsWith("phone"), "keyboard flow checked on desktop");
  await onboard(page, `Hana ${info.project.name}`);
  await page.goto("/");
  await page.getByRole("heading", { level: 1 }).waitFor();
  await page.keyboard.press("Tab");
  const active = () => page.evaluate(() => document.activeElement?.textContent?.trim() ?? "");
  expect(await active()).toBe("Skip to content");
  await page.goto("/tasks");
  await page.getByRole("button", { name: "New task" }).click();
  await expect.poll(() => page.evaluate(() => (document.activeElement as HTMLInputElement | null)?.id)).toBe(await page.getByLabel("What needs doing?").getAttribute("id"));
  for (let i = 0; i < 40; i++) await page.keyboard.press("Tab");
  const inDialog = await page.evaluate(() => !!document.activeElement?.closest("dialog"));
  expect(inDialog).toBe(true);
  await page.keyboard.press("Escape");
  await expect.poll(active).toContain("New task");
});
