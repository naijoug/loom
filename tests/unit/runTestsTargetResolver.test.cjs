const test = require("node:test");
const assert = require("node:assert/strict");
const { mkdtempSync, mkdirSync, rmSync, writeFileSync } = require("node:fs");
const { join, relative } = require("node:path");
const { tmpdir } = require("node:os");

let modulePromise;
function loadRunner() {
  modulePromise ??= import("../../scripts/run-tests.mjs");
  return modulePromise;
}

function withFixture(callback) {
  const root = mkdtempSync(join(tmpdir(), "loom-run-tests-"));
  try {
    mkdirSync(join(root, "tests", "unit", "nested"), { recursive: true });
    writeFileSync(join(root, "tests", "unit", "alpha.test.cjs"), "");
    writeFileSync(join(root, "tests", "unit", "nested", "beta.test.cjs"), "");
    writeFileSync(join(root, "tests", "unit", "notes.txt"), "");
    return callback(root);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

test("findTestsInDirectory recursively returns sorted .test.cjs files", async () => {
  const { findTestsInDirectory } = await loadRunner();
  withFixture((root) => {
    const files = findTestsInDirectory(join(root, "tests", "unit")).map((file) => relative(root, file));
    assert.deepEqual(files, [
      "tests/unit/alpha.test.cjs",
      "tests/unit/nested/beta.test.cjs",
    ]);
  });
});

test("resolveRequestedTestFiles defaults to tests/unit", async () => {
  const { resolveRequestedTestFiles } = await loadRunner();
  withFixture((root) => {
    const files = resolveRequestedTestFiles([], root).map((file) => relative(root, file));
    assert.deepEqual(files, [
      "tests/unit/alpha.test.cjs",
      "tests/unit/nested/beta.test.cjs",
    ]);
  });
});

test("resolveRequestedTestFiles supports pnpm separator, files, directories, and absolute targets", async () => {
  const { resolveRequestedTestFiles } = await loadRunner();
  withFixture((root) => {
    const absoluteFile = join(root, "tests", "unit", "nested", "beta.test.cjs");
    const files = resolveRequestedTestFiles([
      "--",
      "tests/unit/alpha.test.cjs",
      absoluteFile,
    ], root).map((file) => relative(root, file));
    assert.deepEqual(files, [
      "tests/unit/alpha.test.cjs",
      "tests/unit/nested/beta.test.cjs",
    ]);

    const directoryFiles = resolveRequestedTestFiles(["tests/unit"], root).map((file) => relative(root, file));
    assert.deepEqual(directoryFiles, [
      "tests/unit/alpha.test.cjs",
      "tests/unit/nested/beta.test.cjs",
    ]);
  });
});

test("resolveRequestedTestFiles rejects unsupported or empty targets", async () => {
  const { resolveRequestedTestFiles } = await loadRunner();
  withFixture((root) => {
    mkdirSync(join(root, "empty"));
    assert.throws(() => resolveRequestedTestFiles(["package.json"], root), /Test target does not exist/);
    writeFileSync(join(root, "package.json"), "{}");
    assert.throws(() => resolveRequestedTestFiles(["package.json"], root), /Unsupported test target/);
    assert.throws(() => resolveRequestedTestFiles(["empty"], root), /contains no \.test\.cjs files/);
  });
});
