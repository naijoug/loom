const test = require("node:test");
const assert = require("node:assert/strict");
const {
  effectiveThemeForMode,
  isThemeMode,
  resolveThemeMode,
} = require("../../.tmp/test-build/src/contexts/ThemeContext.js");

test("resolveThemeMode prefers settings over the legacy localStorage theme", () => {
  assert.equal(resolveThemeMode("system", "dark"), "system");
  assert.equal(resolveThemeMode("light", "dark"), "light");
});

test("resolveThemeMode falls back to legacy light/dark and then system", () => {
  assert.equal(resolveThemeMode(undefined, "dark"), "dark");
  assert.equal(resolveThemeMode(undefined, "blue"), "system");
});

test("effectiveThemeForMode maps system mode to the current system theme", () => {
  assert.equal(effectiveThemeForMode("system", "dark"), "dark");
  assert.equal(effectiveThemeForMode("light", "dark"), "light");
});

test("isThemeMode accepts only supported theme modes", () => {
  assert.equal(isThemeMode("light"), true);
  assert.equal(isThemeMode("system"), true);
  assert.equal(isThemeMode("auto"), false);
});
