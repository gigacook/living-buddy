import { readFileSync } from "node:fs";
import { join } from "node:path";
import { contrast } from "./contrast";

const css = readFileSync(join(__dirname, "../styles/tokens.css"), "utf8");

function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector);
  const body = css.slice(css.indexOf("{", start) + 1, css.indexOf("}", start));
  const vars: Record<string, string> = {};
  for (const m of body.matchAll(/--([\w-]+):\s*(#[0-9a-fA-F]{6})/g)) vars[m[1]] = m[2];
  return vars;
}

const themes = { light: block(":root {"), dark: block(':root[data-theme="dark"]') };
const cats = ["home", "errands", "people", "school", "work", "personal"];

describe.each(Object.entries(themes))("%s theme contrast (WCAG AA)", (_, v) => {
  it("body and muted text on all surfaces", () => {
    for (const bg of ["bg", "surface", "surface-2"]) {
      expect(contrast(v.text, v[bg])).toBeGreaterThanOrEqual(4.5);
      expect(contrast(v["text-muted"], v[bg])).toBeGreaterThanOrEqual(4.5);
    }
  });
  it("buttons and status colors", () => {
    expect(contrast(v["accent-text"], v.accent)).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v["accent-soft-text"], v["accent-soft"])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v.success, v["success-soft"])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v.warning, v["warning-soft"])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v.danger, v["danger-soft"])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v.focus, v.bg)).toBeGreaterThanOrEqual(3);
    expect(contrast(v["line-strong"], v.surface)).toBeGreaterThanOrEqual(3);
  });
  it.each(cats)("category %s label and text on its pastel surface", (c) => {
    expect(contrast(v[`cat-${c}-fg`], v[`cat-${c}-bg`])).toBeGreaterThanOrEqual(4.5);
    expect(contrast(v.text, v[`cat-${c}-bg`])).toBeGreaterThanOrEqual(4.5);
  });
});

it("dark media-query block matches the explicit dark theme", () => {
  const media = css.slice(css.indexOf("@media (prefers-color-scheme: dark)"), css.indexOf(':root[data-theme="dark"]'));
  for (const [k, val] of Object.entries(themes.dark)) expect(media).toContain(`--${k}: ${val}`);
});
