#!/usr/bin/env node

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";

const projectRoot = path.resolve(new URL("..", import.meta.url).pathname);
const checkerPath = path.join(projectRoot, "scripts", "check-release-docs.mjs");
const sourceReleaseDir = path.join(projectRoot, "docs", "release");
const tempRoot = fs.mkdtempSync(path.join(os.tmpdir(), "loom-release-docs-checker-"));

function makeFixture(name) {
  const fixtureRoot = path.join(tempRoot, name);
  const fixtureReleaseDir = path.join(fixtureRoot, "docs", "release");
  fs.mkdirSync(path.dirname(fixtureReleaseDir), { recursive: true });
  fs.cpSync(sourceReleaseDir, fixtureReleaseDir, { recursive: true });
  return { fixtureRoot, fixtureReleaseDir };
}

function runChecker(fixtureRoot) {
  return spawnSync(process.execPath, [checkerPath], {
    cwd: projectRoot,
    env: {
      ...process.env,
      LOOM_RELEASE_DOCS_PROJECT_ROOT: fixtureRoot,
    },
    encoding: "utf8",
  });
}

function assertIncludes(haystack, needle, label) {
  if (!haystack.includes(needle)) {
    throw new Error(`${label}: expected output to include ${JSON.stringify(needle)}\nOutput:\n${haystack}`);
  }
}

function expectPass(name, mutate) {
  const { fixtureRoot, fixtureReleaseDir } = makeFixture(name);
  mutate?.(fixtureReleaseDir);
  const result = runChecker(fixtureRoot);
  if (result.status !== 0) {
    throw new Error(`${name}: expected checker to pass\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  }
  assertIncludes(result.stdout, "Release docs check passed", name);
}

function expectFail(name, mutate, expectedMessage) {
  const { fixtureRoot, fixtureReleaseDir } = makeFixture(name);
  mutate(fixtureReleaseDir);
  const result = runChecker(fixtureRoot);
  if (result.status === 0) {
    throw new Error(`${name}: expected checker to fail\nSTDOUT:\n${result.stdout}\nSTDERR:\n${result.stderr}`);
  }
  assertIncludes(`${result.stdout}\n${result.stderr}`, expectedMessage, name);
}

function readChecklist(fixtureReleaseDir) {
  return fs.readFileSync(path.join(fixtureReleaseDir, "beta-release-review-checklist.md"), "utf8");
}

function writeChecklist(fixtureReleaseDir, text) {
  fs.writeFileSync(path.join(fixtureReleaseDir, "beta-release-review-checklist.md"), text);
}

try {
  expectPass("baseline");

  expectFail(
    "wrong-checklist-status",
    (fixtureReleaseDir) => {
      writeChecklist(
        fixtureReleaseDir,
        readChecklist(fixtureReleaseDir).replace(
          "| Credential preflight | `xcrun notarytool history --keychain-profile loom-beta-notary` 可读取 profile | Hold |",
          "| Credential preflight | `xcrun notarytool history --keychain-profile loom-beta-notary` 可读取 profile | Pass |",
        ),
      );
    },
    'review table gate "Credential preflight" has status "Pass", expected "Hold"',
  );

  expectFail(
    "missing-checklist-evidence",
    (fixtureReleaseDir) => {
      writeChecklist(
        fixtureReleaseDir,
        readChecklist(fixtureReleaseDir).replace("、`docs/release/beta-safety-notes.md`", ""),
      );
    },
    'review table gate "Scope / safety" missing evidence "docs/release/beta-safety-notes.md"',
  );

  expectFail(
    "local-absolute-path",
    (fixtureReleaseDir) => {
      fs.appendFileSync(
        path.join(fixtureReleaseDir, "README.md"),
        "\nTemporary local debug path: `/Users/example/loom/dist/Loom.dmg`\n",
      );
    },
    "contains forbidden local absolute path pattern",
  );

  expectFail(
    "broken-markdown-link",
    (fixtureReleaseDir) => {
      fs.appendFileSync(
        path.join(fixtureReleaseDir, "README.md"),
        "\nBroken release reference: [missing](missing-release-doc.md)\n",
      );
    },
    "broken markdown link target missing-release-doc.md",
  );

  expectFail(
    "missing-required-release-doc",
    (fixtureReleaseDir) => {
      fs.rmSync(path.join(fixtureReleaseDir, "notarized-dmg-gate.md"));
    },
    "docs/release/notarized-dmg-gate.md: required release doc is missing",
  );

  expectFail(
    "broken-backtick-reference",
    (fixtureReleaseDir) => {
      fs.appendFileSync(
        path.join(fixtureReleaseDir, "README.md"),
        "\nBroken backtick release doc: `docs/release/missing-release-doc.md`\n",
      );
    },
    "broken backtick release doc reference docs/release/missing-release-doc.md",
  );

  expectFail(
    "short-checklist-stop-rule",
    (fixtureReleaseDir) => {
      writeChecklist(
        fixtureReleaseDir,
        readChecklist(fixtureReleaseDir).replace(
          "| Feedback path | `docs/release/beta-feedback-template.md` 可直接复制给试用者 | Review | 反馈分类或必要证据字段缺失时停止 |",
          "| Feedback path | `docs/release/beta-feedback-template.md` 可直接复制给试用者 | Review | 停 |",
        ),
      );
    },
    'review table gate "Feedback path" has empty stop rule',
  );

  expectFail(
    "missing-record-hold-gate",
    (fixtureReleaseDir) => {
      const recordPath = path.join(fixtureReleaseDir, "diagnostic-bundle-smoke-record.md");
      fs.writeFileSync(
        recordPath,
        fs
          .readFileSync(recordPath, "utf8")
          .replace("Diagnostic bundle beta gate: Hold", "Diagnostic bundle beta gate: Pass"),
      );
    },
    'missing record gate phrase "Diagnostic bundle beta gate: Hold"',
  );

  console.log("Release docs checker fixture tests passed.");
} finally {
  fs.rmSync(tempRoot, { recursive: true, force: true });
}
