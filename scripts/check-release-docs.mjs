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
  },
  {
    gate: "Feedback path",
    status: "Review",
    evidence: ["docs/release/beta-feedback-template.md"],
  },
  {
    gate: "Artifact identity",
    status: "Wait",
    evidence: ["commit", "SHA-256", "size", "build command"],
  },
  {
    gate: "Credential preflight",
    status: "Hold",
    evidence: ["xcrun notarytool history --keychain-profile loom-beta-notary"],
  },
  {
    gate: "Developer ID identity",
    status: "Review",
    evidence: ["security find-identity -v -p codesigning"],
  },
  {
    gate: "Notarized DMG gate",
    status: "Wait",
    evidence: ["hdiutil verify", "codesign", "spctl", "staple validate"],
  },
  {
    gate: "Maintainer local smoke",
    status: "Pass",
    evidence: ["pnpm smoke:desktop"],
  },
  {
    gate: "First-run beta smoke",
    status: "Wait",
    evidence: ["docs/release/beta-smoke.md", "docs/release/beta-first-run-smoke-record.md"],
  },
  {
    gate: "Diagnostic bundle smoke",
    status: "Wait",
    evidence: ["docs/release/diagnostic-bundle-smoke.md", "docs/release/diagnostic-bundle-smoke-record.md"],
  },
  {
    gate: "Install / uninstall smoke",
    status: "Wait",
    evidence: [
      "docs/release/macos-install.md",
      "docs/release/local-data-and-uninstall.md",
      "docs/release/install-uninstall-smoke-record.md",
    ],
  },
  {
    gate: "Privacy / account boundary",
    status: "Review",
    evidence: ["docs/release/privacy-note.md", "docs/release/agent-account-boundary.md"],
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
