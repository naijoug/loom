#!/usr/bin/env node

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const projectRoot = path.resolve(new URL("..", import.meta.url).pathname);
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
}

if (failures.length > 0) {
  console.error("Release docs check failed:");
  for (const failure of failures) {
    console.error(`- ${failure}`);
  }
  process.exit(1);
}

console.log(`Release docs check passed (${markdownFiles.length} markdown files).`);
