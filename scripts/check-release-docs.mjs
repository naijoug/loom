#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const defaultProjectRoot = path.resolve(new URL("..", import.meta.url).pathname);
const projectRoot = process.env.LOOM_RELEASE_DOCS_PROJECT_ROOT
  ? path.resolve(process.env.LOOM_RELEASE_DOCS_PROJECT_ROOT)
  : defaultProjectRoot;
const releaseDir = path.join(projectRoot, "docs", "release");

const requiredFiles = [
  "README.md",
  "beta-scope.md",
  "beta-safety-notes.md",
  "beta-feedback-template.md",
  "release-build-record.md",
  "artifact-integrity-check.md",
  "signing-gate-decision.md",
  "signing-repair-probe.md",
  "developer-id-notarization-probe.md",
  "notarization-credential-preflight.md",
  "notarized-dmg-gate.md",
  "beta-release-review-checklist.md",
  "local-desktop-smoke-record.md",
  "install-uninstall-smoke-record.md",
  "beta-first-run-smoke-record.md",
  "beta-smoke.md",
  "diagnostic-bundle-smoke.md",
  "diagnostic-bundle-smoke-record.md",
  "macos-install.md",
  "local-data-and-uninstall.md",
  "privacy-note.md",
  "agent-account-boundary.md",
  "diagnostic-bundle-review.md",
  "diagnostic-bundle-engineering-proof.md",
];

const forbiddenLocalPathPatterns = [
  /\/Users\//,
  /file:\/\/\/Users\//,
  /\/private\/var\/folders\//,
];

const requiredChecklistRows = [
  {
    gate: "Scope / safety",
    evidence: ["docs/release/beta-scope.md", "docs/release/beta-safety-notes.md"],
    stopRuleEvidence: ["安全边界"],
  },
  {
    gate: "Feedback path",
    evidence: ["docs/release/beta-feedback-template.md"],
    stopRuleEvidence: ["反馈", "证据"],
  },
  {
    gate: "Artifact identity",
    requiresRecordedPass: true,
    evidence: ["commit", "SHA-256", "size", "build command"],
    stopRuleEvidence: ["checksum", "新候选"],
  },
  {
    gate: "Credential preflight",
    requiresRecordedPass: true,
    evidence: ["xcrun notarytool history --keychain-profile loom-beta-notary"],
    stopRuleEvidence: ["No Keychain password item found"],
  },
  {
    gate: "Developer ID identity",
    evidence: ["security find-identity -v -p codesigning"],
    stopRuleEvidence: ["identity"],
  },
  {
    gate: "Notarized DMG gate",
    requiresRecordedPass: true,
    evidence: ["hdiutil verify", "codesign", "spctl", "staple validate"],
    stopRuleEvidence: ["gate fail", "分发"],
  },
  {
    gate: "Maintainer local smoke",
    evidence: ["pnpm smoke:desktop"],
    stopRuleEvidence: ["不能替代 notarization", "Gatekeeper"],
  },
  {
    gate: "First-run beta smoke",
    requiresRecordedPass: true,
    evidence: ["docs/release/beta-smoke.md", "docs/release/beta-first-run-smoke-record.md"],
    stopRuleEvidence: ["同一 artifact", "旧 artifact"],
  },
  {
    gate: "Diagnostic bundle smoke",
    requiresRecordedPass: true,
    evidence: [
      "pnpm smoke:diagnostics",
      "docs/release/diagnostic-bundle-smoke.md",
      "docs/release/diagnostic-bundle-smoke-record.md",
    ],
    stopRuleEvidence: ["诊断包", "人工复核"],
  },
  {
    gate: "Install / uninstall smoke",
    requiresRecordedPass: true,
    evidence: [
      "docs/release/macos-install.md",
      "docs/release/local-data-and-uninstall.md",
      "docs/release/install-uninstall-smoke-record.md",
    ],
    stopRuleEvidence: ["同一 artifact", "安装 / 卸载"],
  },
  {
    gate: "Privacy / account boundary",
    evidence: ["docs/release/privacy-note.md", "docs/release/agent-account-boundary.md"],
    stopRuleEvidence: ["真实 Agent 账号", "外发数据"],
  },
];

const requiredRecordGatePhrases = [
  {
    file: "release-build-record.md",
    phrases: [
      "Remaining release gates",
      "If the DMG is rebuilt, update the commit, environment snapshot, checksum and size in this file",
    ],
  },
  {
    file: "signing-gate-decision.md",
    phrases: [
      "不要求试用者绕过 Gatekeeper",
      "下一份候选产物必须先解决签名 gate",
    ],
  },
  {
    file: "artifact-integrity-check.md",
    phrases: [
      "should **not** be promoted as a public Beta download",
      "Run `docs/release/beta-smoke.md` against the exact artifact",
    ],
  },
  {
    file: "notarized-dmg-gate.md",
    phrases: [
      "不要扩大 Beta 分发",
      "严格 `codesign`、Gatekeeper assessment 和 staple validate 都通过",
    ],
  },
  {
    file: "beta-first-run-smoke-record.md",
    passGate: "First-run beta smoke",
    passPattern: /^(?:- )?(?:\*\*)?Beta gate: pass(?:\*\*)?\s*$/im,
    phrases: [
      "不能替代本记录",
    ],
  },
  {
    file: "diagnostic-bundle-smoke-record.md",
    passGate: "Diagnostic bundle smoke",
    passPattern: /^(?:- )?(?:\*\*)?Diagnostic bundle beta gate: Pass(?:\*\*)?\s*$/im,
    phrases: [
      "在本记录出现至少一条 `Beta gate: pass` 前",
      "不能把诊断包 UI smoke 视为已满足邀请制 Beta 分发门禁",
      "这条门禁不能解除 `loom-beta-notary` credential",
    ],
  },
  {
    file: "install-uninstall-smoke-record.md",
    passGate: "Install / uninstall smoke",
    passPattern: /^(?:- )?(?:\*\*)?Install \/ uninstall beta gate: Pass(?:\*\*)?\s*$/im,
    phrases: [
      "不要求试用者绕过 Gatekeeper",
      "这份记录不能单独解锁邀请制 Beta 或公开分发",
    ],
  },
];

const failures = [];

function rel(filePath) {
  return path.relative(projectRoot, filePath);
}

function fail(filePath, message) {
  failures.push(`${rel(filePath)}: ${message}`);
}

function fileExists(filePath) {
  try {
    return fs.statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function directoryExists(dirPath) {
  try {
    return fs.statSync(dirPath).isDirectory();
  } catch {
    return false;
  }
}

function parseMarkdownTableRows(markdown) {
  const rows = new Map();
  for (const line of markdown.split("\n")) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("|")) {
      continue;
    }
    const cells = trimmed
      .split("|")
      .slice(1, -1)
      .map((cell) => cell.trim());
    if (cells.length < 4 || cells[0] === "Gate" || /^-+$/.test(cells[0])) {
      continue;
    }
    rows.set(cells[0], {
      evidence: cells[1],
      status: cells[2],
      stopRule: cells[3],
    });
  }
  return rows;
}

function resolveMarkdownTarget(filePath, target) {
  const cleanTarget = target.split("#", 1)[0];
  if (!cleanTarget || cleanTarget.startsWith("http://") || cleanTarget.startsWith("https://")) {
    return null;
  }
  if (cleanTarget.startsWith("mailto:") || cleanTarget.startsWith("#")) {
    return null;
  }
  if (path.isAbsolute(cleanTarget)) {
    return null;
  }
  if (!cleanTarget.endsWith(".md")) {
    return null;
  }
  return path.resolve(path.dirname(filePath), cleanTarget);
}

function listMarkdownFiles() {
  return fs
    .readdirSync(releaseDir)
    .filter((entry) => entry.endsWith(".md"))
    .map((entry) => path.join(releaseDir, entry));
}

function checkRequiredFiles() {
  for (const fileName of requiredFiles) {
    const filePath = path.join(releaseDir, fileName);
    if (!fileExists(filePath)) {
      failures.push(`docs/release/${fileName}: required release doc is missing`);
    }
  }
}

function checkReleaseDirectory() {
  if (directoryExists(releaseDir)) {
    return true;
  }
  failures.push("docs/release: release docs directory is missing");
  return false;
}

function checkMarkdownFile(filePath) {
  const text = fs.readFileSync(filePath, "utf8");

  for (const pattern of forbiddenLocalPathPatterns) {
    if (pattern.test(text)) {
      fail(filePath, `contains forbidden local absolute path pattern ${pattern}`);
    }
  }

  for (const match of text.matchAll(/\[[^\]]+\]\(([^)]+)\)/g)) {
    const targetPath = resolveMarkdownTarget(filePath, match[1].trim());
    if (targetPath && !fileExists(targetPath)) {
      fail(filePath, `broken markdown link target ${match[1].trim()}`);
    }
  }

  for (const match of text.matchAll(/`((?:docs\/release\/)?[a-z0-9-]+\.md)`/g)) {
    const reference = match[1];
    const targetPath = reference.startsWith("docs/release/")
      ? path.join(projectRoot, reference)
      : path.join(releaseDir, reference);
    if (!fileExists(targetPath)) {
      fail(filePath, `broken backtick release doc reference ${reference}`);
    }
  }
}

function checkMarkdownFiles(markdownFiles) {
  for (const filePath of markdownFiles) {
    checkMarkdownFile(filePath);
  }
}

function checkRequiredChecklistPhrases(checklistPath, checklist) {
  for (const phrase of [
    "Credential preflight",
    "Notarized DMG gate",
    "Install / uninstall smoke",
    "First-run beta smoke",
    "Diagnostic bundle smoke",
    "credential 缺失期间不要重打普通 Beta DMG",
    "不要要求试用者绕过 Gatekeeper",
    "不要把本机 smoke pass 改写为分发 pass",
    "只有 `Credential preflight`、`DMG checksum / size`、`hdiutil verify`、`Strict codesign`、`spctl assessment`、`stapler validate`、`Install / uninstall smoke`、`First-run beta smoke` 和 `Diagnostic bundle smoke` 都有针对同一候选物的 pass 证据时",
    "才允许把 `Distribution decision` 从 `Hold` 改为 `Invite-only`",
  ]) {
    if (!checklist.includes(phrase)) {
      fail(checklistPath, `missing release gate phrase ${JSON.stringify(phrase)}`);
    }
  }
}

// Historical Hold/Wait snapshots are evidence, not permanent policy. A new
// candidate may progress only with explicit, candidate-bound review records.
function checkRecordedPass(sourcePath, gate) {
  const manifestPath = path.join(releaseDir, "release-evidence.json");
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch {
    fail(sourcePath, `gate ${JSON.stringify(gate)} Pass requires valid release-evidence.json`);
    return;
  }
  const candidate = manifest?.candidate;
  if (manifest?.schemaVersion !== 1
      || !/^[a-f0-9]{7,40}$/i.test(candidate?.commit ?? "")
      || !/^[a-f0-9]{64}$/i.test(candidate?.sha256 ?? "")
      || !Number.isSafeInteger(candidate?.sizeBytes) || candidate.sizeBytes <= 0
      || typeof candidate?.buildCommand !== "string" || !candidate.buildCommand.trim()
      || typeof candidate?.artifact !== "string" || !candidate.artifact.trim()) {
    fail(sourcePath, "release evidence needs schemaVersion 1 and candidate commit/SHA-256/size/build command/artifact");
    return;
  }
  const evidence = manifest.gates?.[gate];
  if (evidence?.status !== "Pass" || evidence.candidateSha256 !== candidate.sha256
      || typeof evidence.reviewer !== "string" || !evidence.reviewer.trim()
      || typeof evidence.checkedAt !== "string" || !Number.isFinite(Date.parse(evidence.checkedAt))
      || typeof evidence.record !== "string" || !evidence.record.endsWith(".md")) {
    fail(sourcePath, `gate ${JSON.stringify(gate)} requires a reviewed Pass record for the same candidate`);
    return;
  }
  const recordPath = path.resolve(releaseDir, evidence.record);
  let record;
  try {
    const realRoot = fs.realpathSync(releaseDir);
    const realRecord = fs.realpathSync(recordPath);
    const relative = path.relative(realRoot, realRecord);
    if (path.isAbsolute(evidence.record) || relative === ".." || relative.startsWith(`..${path.sep}`)
        || path.isAbsolute(relative) || !fs.statSync(realRecord).isFile()) {
      throw new Error("record outside release directory");
    }
    record = fs.readFileSync(realRecord, "utf8");
  } catch {
    fail(sourcePath, `gate ${JSON.stringify(gate)} evidence record must exist inside docs/release`);
    return;
  }
  for (const expected of [gate, candidate.commit, candidate.sha256, "Result: Pass"]) {
    if (!record.includes(expected)) {
      fail(sourcePath, `gate ${JSON.stringify(gate)} record missing candidate/result evidence ${JSON.stringify(expected)}`);
    }
  }
  // Apply link/path hygiene to nested records as well as top-level Markdown.
  checkMarkdownFile(recordPath);
  if (forbiddenLocalPathPatterns.some((pattern) => pattern.test(JSON.stringify(manifest)))) {
    fail(manifestPath, "contains forbidden local absolute path in release evidence");
  }
}

function checkDistributionDecision(checklistPath, checklist, rows) {
  const decisions = [...checklist.matchAll(/^\*\*Distribution decision: ([^*\n]+)\*\*\s*$/gm)];
  if (decisions.length !== 1 || !["Hold", "Invite-only"].includes(decisions[0][1])) {
    fail(checklistPath, "expected one current Distribution decision: Hold or Invite-only; Public requires a separate release review");
    return;
  }
  if (decisions[0][1] === "Invite-only") {
    for (const required of requiredChecklistRows) {
      if (rows.get(required.gate)?.status !== "Pass") {
        fail(checklistPath, `Invite-only requires gate ${JSON.stringify(required.gate)} Pass`);
      }
      // Distribution needs fresh same-candidate review of every gate, including
      // environment and document gates that can otherwise be checked separately.
      checkRecordedPass(checklistPath, required.gate);
    }
  }
}

function checkChecklistRow(checklistPath, checklistRows, requiredRow) {
  const row = checklistRows.get(requiredRow.gate);
  if (!row) {
    fail(checklistPath, `missing review table gate ${JSON.stringify(requiredRow.gate)}`);
    return;
  }
  if (!["Pass", "Wait", "Hold", "Fail", "Review"].includes(row.status)) {
    fail(checklistPath, `review table gate ${JSON.stringify(requiredRow.gate)} has invalid status ${JSON.stringify(row.status)}`);
  }
  if (row.status === "Pass" && requiredRow.requiresRecordedPass) {
    checkRecordedPass(checklistPath, requiredRow.gate);
  }
  for (const evidence of requiredRow.evidence) {
    if (!row.evidence.includes(evidence)) {
      fail(
        checklistPath,
        `review table gate ${JSON.stringify(requiredRow.gate)} missing evidence ${JSON.stringify(
          evidence,
        )}`,
      );
    }
  }
  if (row.stopRule.length < 8) {
    fail(checklistPath, `review table gate ${JSON.stringify(requiredRow.gate)} has empty stop rule`);
  }
  for (const stopRuleEvidence of requiredRow.stopRuleEvidence) {
    if (!row.stopRule.includes(stopRuleEvidence)) {
      fail(
        checklistPath,
        `review table gate ${JSON.stringify(requiredRow.gate)} missing stop rule evidence ${JSON.stringify(
          stopRuleEvidence,
        )}`,
      );
    }
  }
}

function checkChecklist() {
  const checklistPath = path.join(releaseDir, "beta-release-review-checklist.md");
  if (!fileExists(checklistPath)) {
    return;
  }

  const checklist = fs.readFileSync(checklistPath, "utf8");
  checkRequiredChecklistPhrases(checklistPath, checklist);

  const checklistRows = parseMarkdownTableRows(checklist);
  for (const requiredRow of requiredChecklistRows) {
    checkChecklistRow(checklistPath, checklistRows, requiredRow);
  }
  checkDistributionDecision(checklistPath, checklist, checklistRows);
}

function checkRecordGatePhrases() {
  for (const recordGate of requiredRecordGatePhrases) {
    const recordPath = path.join(releaseDir, recordGate.file);
    if (!fileExists(recordPath)) {
      continue;
    }
    const text = fs.readFileSync(recordPath, "utf8");
    if (recordGate.passPattern?.test(text)) {
      checkRecordedPass(recordPath, recordGate.passGate);
    }
    for (const phrase of recordGate.phrases) {
      if (!text.includes(phrase)) {
        fail(recordPath, `missing record gate phrase ${JSON.stringify(phrase)}`);
      }
    }
  }
}

function reportFailures() {
  if (failures.length === 0) {
    return;
  }

  console.error("Release docs check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

function main() {
  if (!checkReleaseDirectory()) {
    reportFailures();
    return;
  }

  checkRequiredFiles();
  const markdownFiles = listMarkdownFiles();
  checkMarkdownFiles(markdownFiles);
  checkChecklist();
  checkRecordGatePhrases();
  reportFailures();
  console.log(`Release docs check passed (${markdownFiles.length} markdown files).`);
}

main();
