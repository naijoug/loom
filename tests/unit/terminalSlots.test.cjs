const test = require("node:test");
const assert = require("node:assert/strict");
const {
  applyTerminalSlotDraft,
  buildDefaultSlots,
  draftFromTerminalSlot,
  resolveTerminalSlotCwd,
  terminalSlotFromDraft,
} = require("../../.tmp/test-build/src/utils/terminalSlots.js");

function project(overrides = {}) {
  return {
    id: "project-1",
    path: "/repo",
    name: "Repo",
    detectedStacks: [],
    suggestedCommands: [],
    isGitRepository: true,
    hasUncommittedChanges: false,
    loomDirReady: true,
    schemaVersion: 1,
    ...overrides,
  };
}

test("buildDefaultSlots prefers detected preview and validation commands", () => {
  const slots = buildDefaultSlots(project({
    suggestedCommands: ["pnpm build", "pnpm dev", "pnpm test"],
  }));

  assert.equal(slots[0].kind, "preview");
  assert.equal(slots[0].command, "pnpm dev");
  assert.equal(slots[1].kind, "validation");
  assert.equal(slots[1].command, "pnpm test");
});

test("buildDefaultSlots falls back from project stacks", () => {
  const slots = buildDefaultSlots(project({ detectedStacks: ["Tauri"] }));

  assert.equal(slots[1].command, "cargo check --manifest-path src-tauri/Cargo.toml");
});

test("terminalSlotFromDraft trims values and rejects incomplete drafts", () => {
  assert.equal(terminalSlotFromDraft({ id: null, name: "  ", command: "pnpm test", kind: "validation", cwd: "" }), null);

  const slot = terminalSlotFromDraft({
    id: "slot-1",
    name: " Test ",
    command: " pnpm test ",
    kind: "validation",
    cwd: " web ",
  });
  assert.deepEqual(slot, {
    id: "slot-1",
    name: "Test",
    command: "pnpm test",
    kind: "validation",
    cwd: "web",
  });
});

test("applyTerminalSlotDraft updates or appends slots", () => {
  const existing = [{ id: "slot-1", name: "Test", command: "pnpm test", kind: "validation" }];

  assert.deepEqual(
    applyTerminalSlotDraft(existing, { id: "slot-1", name: "Build", command: "pnpm build", kind: "validation", cwd: "" }),
    [{ id: "slot-1", name: "Build", command: "pnpm build", kind: "validation", cwd: undefined }],
  );

  const appended = applyTerminalSlotDraft(existing, { id: null, name: "Preview", command: "pnpm dev", kind: "preview", cwd: "" });
  assert.equal(appended.length, 2);
  assert.equal(appended[1].name, "Preview");
});

test("draftFromTerminalSlot and resolveTerminalSlotCwd share slot shape", () => {
  const slot = { id: "slot-1", name: "Web test", command: "pnpm test", kind: "validation", cwd: "web" };

  assert.deepEqual(draftFromTerminalSlot(slot), {
    id: "slot-1",
    name: "Web test",
    command: "pnpm test",
    kind: "validation",
    cwd: "web",
  });
  assert.equal(resolveTerminalSlotCwd("/repo", slot), "/repo/web");
  assert.equal(resolveTerminalSlotCwd("/repo", { cwd: undefined }), "/repo");
});
