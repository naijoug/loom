const test = require("node:test"),
  assert = require("node:assert/strict");
const { createStartupHandshake, startupFailed } = require("../../.tmp/test-build/src/appStartup.js");
const { JSDOM } = require("jsdom");
test("startup readiness uses the original document generation and StrictMode does not duplicate it", async () => {
  let resolve,
    calls = [];
  const h = createStartupHandshake({
    context: () => new Promise((r) => (resolve = r)),
    report: async (g, status) => {
      calls.push([g, status]);
      return true;
    },
  });
  const ready1 = h.ready(),
    ready2 = h.ready();
  assert.equal(ready1, ready2);
  assert.equal(calls.length, 0);
  resolve(7);
  await h.prepared;
  assert.equal(await ready1, true);
  assert.deepEqual(calls, [[7, "ready"]]);
  await h.failed();
  assert.deepEqual(calls[1], [7, "failed"]);
});
test("a stale native acknowledgement stays rejected instead of retrying with a new generation", async () => {
  let contexts = 0;
  const h = createStartupHandshake({
    context: async () => {
      contexts++;
      return 3;
    },
    report: async () => false,
  });
  await h.prepared;
  assert.equal(await h.ready(), false);
  assert.equal(await h.ready(), false);
  assert.equal(contexts, 1);
});
test("a bootstrap failure produces a visible actionable fallback without rendering React", () => {
  const old = global.document;
  const dom = new JSDOM(
    '<section><span id="loom-startup-status" role="status">Opening</span><button id="loom-startup-retry" hidden>Retry</button></section>',
  );
  global.document = dom.window.document;
  try {
    startupFailed();
    assert.equal(document.querySelector("[role=alert]").textContent.includes("could not open"), true);
    assert.equal(document.querySelector("button").hidden, false);
  } finally {
    global.document = old;
    dom.window.close();
  }
});
