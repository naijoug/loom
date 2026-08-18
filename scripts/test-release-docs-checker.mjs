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

function assertDiagnosticSmokeScriptContract() {
  const packageJsonPath = path.join(projectRoot, "package.json");
  const diagnosticScriptPath = path.join(projectRoot, "scripts", "diagnostic-bundle-smoke.sh");
  const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));
  const smokeCommand = packageJson.scripts?.["smoke:diagnostics"];

  if (smokeCommand !== "./scripts/diagnostic-bundle-smoke.sh") {
    throw new Error(
      `package.json: expected smoke:diagnostics to run ./scripts/diagnostic-bundle-smoke.sh, got ${JSON.stringify(smokeCommand)}`,
    );
  }

  const diagnosticScript = fs.readFileSync(diagnosticScriptPath, "utf8");
  assertIncludes(diagnosticScript, "set -euo pipefail", "diagnostic smoke script strict mode");
  assertIncludes(
    diagnosticScript,
    "cargo test --manifest-path src-tauri/Cargo.toml diagnostic_bundle --lib",
    "diagnostic smoke script cargo harness",
  );

  const mode = fs.statSync(diagnosticScriptPath).mode;
  if ((mode & 0o111) === 0) {
    throw new Error("scripts/diagnostic-bundle-smoke.sh: expected script to be executable");
  }
}

function readChecklist(fixtureReleaseDir) {
  return fs.readFileSync(path.join(fixtureReleaseDir, "beta-release-review-checklist.md"), "utf8");
}

function writeChecklist(fixtureReleaseDir, text) {
  fs.writeFileSync(path.join(fixtureReleaseDir, "beta-release-review-checklist.md"), text);
}

try {
  assertDiagnosticSmokeScriptContract();
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
    "missing-diagnostic-smoke-command-evidence",
    (fixtureReleaseDir) => {
      writeChecklist(
        fixtureReleaseDir,
        readChecklist(fixtureReleaseDir).replace("`pnpm smoke:diagnostics` 已通过，", ""),
      );
    },
    'review table gate "Diagnostic bundle smoke" missing evidence "pnpm smoke:diagnostics"',
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
    "missing-required-build-record",
    (fixtureReleaseDir) => {
      fs.rmSync(path.join(fixtureReleaseDir, "release-build-record.md"));
    },
    "docs/release/release-build-record.md: required release doc is missing",
  );

  expectFail(
    "missing-release-directory",
    (fixtureReleaseDir) => {
      fs.rmSync(fixtureReleaseDir, { recursive: true, force: true });
    },
    "docs/release: release docs directory is missing",
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
          "| Feedback path | `docs/release/beta-feedback-template.md` 可直接复制给试用者 | Pass | 反馈分类或必要证据字段缺失时停止 |",
          "| Feedback path | `docs/release/beta-feedback-template.md` 可直接复制给试用者 | Pass | 停 |",
        ),
      );
    },
    'review table gate "Feedback path" has empty stop rule',
  );

  expectFail(
    "generic-checklist-stop-rule",
    (fixtureReleaseDir) => {
      writeChecklist(
        fixtureReleaseDir,
        readChecklist(fixtureReleaseDir).replace(
          "| Credential preflight | `xcrun notarytool history --keychain-profile loom-beta-notary` 可读取 profile | Hold | 返回 `No Keychain password item found` 时停止 |",
          "| Credential preflight | `xcrun notarytool history --keychain-profile loom-beta-notary` 可读取 profile | Hold | credential 状态不清时继续等待维护者确认 |",
        ),
      );
    },
    'review table gate "Credential preflight" missing stop rule evidence "No Keychain password item found"',
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

  expectFail(
    "missing-signing-gate-boundary",
    (fixtureReleaseDir) => {
      const recordPath = path.join(fixtureReleaseDir, "signing-gate-decision.md");
      fs.writeFileSync(
        recordPath,
        fs
          .readFileSync(recordPath, "utf8")
          .replace("Gatekeeper hold", "Gatekeeper review")
          .replace("不要求试用者绕过 Gatekeeper", "如有需要再说明 Gatekeeper 处理方式")
          .replace("下一份候选产物必须先解决签名 gate", "下一份候选产物继续复核签名 gate"),
      );
    },
    'missing record gate phrase "Gatekeeper hold"',
  );

  expectFail(
    "missing-first-run-hold-boundary",
    (fixtureReleaseDir) => {
      const recordPath = path.join(fixtureReleaseDir, "beta-first-run-smoke-record.md");
      fs.writeFileSync(
        recordPath,
        fs
          .readFileSync(recordPath, "utf8")
          .replace("不能替代本记录", "不能替代其他记录")
          .replace("邀请制 Beta 分发结论继续保持 hold", "邀请制 Beta 分发结论后续再定"),
      );
    },
    'missing record gate phrase "不能替代本记录"',
  );

  expectFail(
    "missing-install-uninstall-hold-boundary",
    (fixtureReleaseDir) => {
      const recordPath = path.join(fixtureReleaseDir, "install-uninstall-smoke-record.md");
      fs.writeFileSync(
        recordPath,
        fs
          .readFileSync(recordPath, "utf8")
          .replace("Install / uninstall beta gate: Hold", "Install / uninstall beta gate: Pass")
          .replace("这份记录不能单独解锁邀请制 Beta 或公开分发", "这份记录可以作为安装复核证据"),
      );
    },
    'missing record gate phrase "这份记录不能单独解锁邀请制 Beta 或公开分发"',
  );

  expectFail(
    "missing-diagnostic-real-export-boundary",
    (fixtureReleaseDir) => {
      const recordPath = path.join(fixtureReleaseDir, "diagnostic-bundle-smoke-record.md");
      fs.writeFileSync(
        recordPath,
        fs
          .readFileSync(recordPath, "utf8")
          .replace("record template ready; no real desktop export pass yet", "record template ready")
          .replace("尚未针对同一 Beta 候选 artifact 完成真实桌面", "尚未完成")
          .replace("在本记录出现至少一条 `Beta gate: pass` 前", "在后续通过前"),
      );
    },
    'missing record gate phrase "record template ready; no real desktop export pass yet"',
  );

  console.log("Release docs checker fixture tests passed.");
} finally {
  fs.rmSync(tempRoot, { recursive: true, force: true });
}
