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
  "notarization-credential-preflight.md",
  "notarized-dmg-gate.md",
  "beta-release-review-checklist.md",
  "local-desktop-smoke-record.md",
  "install-uninstall-smoke-record.md",
  "beta-first-run-smoke-record.md",
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
    status: "Review",
    evidence: ["docs/release/beta-scope.md", "docs/release/beta-safety-notes.md"],
    stopRuleEvidence: ["安全边界"],
  },
  {
    gate: "Feedback path",
    status: "Review",
    evidence: ["docs/release/beta-feedback-template.md"],
    stopRuleEvidence: ["反馈", "证据"],
  },
  {
    gate: "Artifact identity",
    status: "Wait",
    evidence: ["commit", "SHA-256", "size", "build command"],
    stopRuleEvidence: ["checksum", "新候选"],
  },
  {
    gate: "Credential preflight",
    status: "Hold",
    evidence: ["xcrun notarytool history --keychain-profile loom-beta-notary"],
    stopRuleEvidence: ["No Keychain password item found"],
  },
  {
    gate: "Developer ID identity",
    status: "Review",
    evidence: ["security find-identity -v -p codesigning"],
    stopRuleEvidence: ["identity"],
  },
  {
    gate: "Notarized DMG gate",
    status: "Wait",
    evidence: ["hdiutil verify", "codesign", "spctl", "staple validate"],
    stopRuleEvidence: ["gate fail", "分发"],
  },
  {
    gate: "Maintainer local smoke",
    status: "Pass",
    evidence: ["pnpm smoke:desktop"],
    stopRuleEvidence: ["不能替代 notarization", "Gatekeeper"],
  },
  {
    gate: "First-run beta smoke",
    status: "Wait",
    evidence: ["docs/release/beta-smoke.md", "docs/release/beta-first-run-smoke-record.md"],
    stopRuleEvidence: ["同一 artifact", "旧 artifact"],
  },
  {
    gate: "Diagnostic bundle smoke",
    status: "Wait",
    evidence: ["docs/release/diagnostic-bundle-smoke.md", "docs/release/diagnostic-bundle-smoke-record.md"],
    stopRuleEvidence: ["诊断包", "人工复核"],
  },
  {
    gate: "Install / uninstall smoke",
    status: "Wait",
    evidence: [
      "docs/release/macos-install.md",
      "docs/release/local-data-and-uninstall.md",
      "docs/release/install-uninstall-smoke-record.md",
    ],
    stopRuleEvidence: ["同一 artifact", "安装 / 卸载"],
  },
  {
    gate: "Privacy / account boundary",
    status: "Review",
    evidence: ["docs/release/privacy-note.md", "docs/release/agent-account-boundary.md"],
    stopRuleEvidence: ["真实 Agent 账号", "外发数据"],
  },
];

const requiredRecordGatePhrases = [
  {
    file: "beta-first-run-smoke-record.md",
    phrases: [
      "Beta gate: hold",
      "不能替代本记录",
      "邀请制 Beta 分发结论继续保持 hold",
    ],
  },
  {
    file: "diagnostic-bundle-smoke-record.md",
    phrases: [
      "Diagnostic bundle beta gate: Hold",
      "不能把诊断包 UI smoke 视为已满足邀请制 Beta 分发门禁",
      "这条门禁不能解除 `loom-beta-notary` credential",
    ],
  },
  {
    file: "install-uninstall-smoke-record.md",
    phrases: [
      "Install / uninstall beta gate: Hold",
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

for (const fileName of requiredFiles) {
  const filePath = path.join(releaseDir, fileName);
  if (!fileExists(filePath)) {
    failures.push(`docs/release/${fileName}: required release doc is missing`);
  }
}

const markdownFiles = fs
  .readdirSync(releaseDir)
  .filter((entry) => entry.endsWith(".md"))
  .map((entry) => path.join(releaseDir, entry));

for (const filePath of markdownFiles) {
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

const checklistPath = path.join(releaseDir, "beta-release-review-checklist.md");
if (fileExists(checklistPath)) {
  const checklist = fs.readFileSync(checklistPath, "utf8");
  for (const phrase of [
    "Distribution decision: Hold",
    "Credential preflight",
    "Notarized DMG gate",
    "Install / uninstall smoke",
    "First-run beta smoke",
    "Diagnostic bundle smoke",
  ]) {
    if (!checklist.includes(phrase)) {
      fail(checklistPath, `missing release gate phrase ${JSON.stringify(phrase)}`);
    }
  }

  const checklistRows = parseMarkdownTableRows(checklist);
  for (const requiredRow of requiredChecklistRows) {
    const row = checklistRows.get(requiredRow.gate);
    if (!row) {
      fail(checklistPath, `missing review table gate ${JSON.stringify(requiredRow.gate)}`);
      continue;
    }
    if (row.status !== requiredRow.status) {
      fail(
        checklistPath,
        `review table gate ${JSON.stringify(requiredRow.gate)} has status ${JSON.stringify(
          row.status,
        )}, expected ${JSON.stringify(requiredRow.status)}`,
      );
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
}

for (const recordGate of requiredRecordGatePhrases) {
  const recordPath = path.join(releaseDir, recordGate.file);
  if (!fileExists(recordPath)) {
    continue;
  }
  const text = fs.readFileSync(recordPath, "utf8");
  for (const phrase of recordGate.phrases) {
    if (!text.includes(phrase)) {
      fail(recordPath, `missing record gate phrase ${JSON.stringify(phrase)}`);
    }
  }
}

if (failures.length > 0) {
  console.error("Release docs check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`Release docs check passed (${markdownFiles.length} markdown files).`);
