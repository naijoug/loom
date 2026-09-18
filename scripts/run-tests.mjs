import { existsSync, mkdirSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const root = new URL("..", import.meta.url).pathname;
const outDir = join(root, ".tmp", "test-build");

function run(command, args) {
  const result = spawnSync(command, args, {
    cwd: root,
    stdio: "inherit",
    shell: false,
  });

  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

// TypeScript preserves CSS imports in emitted CommonJS. Empty style stubs let
// Node load isolated React components while jsdom owns layout-independent tests.
function writeStyleStubs(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      writeStyleStubs(path);
    } else if (entry.name.endsWith(".css")) {
      const target = join(outDir, relative(root, path));
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, "");
    }
  }
}

export function findTestsInDirectory(directory) {
  return readdirSync(directory, { withFileTypes: true })
    .flatMap((entry) => {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        return findTestsInDirectory(path);
      }

      return entry.isFile() && entry.name.endsWith(".test.cjs") ? [path] : [];
    })
    .sort();
}

export function resolveRequestedTestTarget(target, rootDirectory = root) {
  const path = isAbsolute(target) ? target : resolve(rootDirectory, target);

  if (!existsSync(path)) {
    throw new Error(`Test target does not exist: ${target}`);
  }

  const stats = statSync(path);
  if (stats.isDirectory()) {
    const files = findTestsInDirectory(path);
    if (files.length === 0) {
      throw new Error(`Test directory contains no .test.cjs files: ${target}`);
    }
    return files;
  }

  if (stats.isFile() && path.endsWith(".test.cjs")) {
    return [path];
  }

  throw new Error(`Unsupported test target: ${target}. Pass a .test.cjs file or a directory containing .test.cjs files.`);
}

export function resolveRequestedTestFiles(args, rootDirectory = root) {
  const testDir = join(rootDirectory, "tests", "unit");
  const requestedTestTargets = args.filter((arg) => arg !== "--");
  return requestedTestTargets.length > 0
    ? requestedTestTargets.flatMap((target) => resolveRequestedTestTarget(target, rootDirectory)).sort()
    : findTestsInDirectory(testDir);
}

function buildTestArtifacts() {
  if (existsSync(outDir)) {
    rmSync(outDir, { recursive: true, force: true });
  }

  mkdirSync(outDir, { recursive: true });
  run("pnpm", ["exec", "tsc", "-p", "tsconfig.test.json"]);
  writeFileSync(join(outDir, "package.json"), '{"type":"commonjs"}\n');
  writeStyleStubs(join(root, "src"));
}

export function main(args = process.argv.slice(2)) {
  buildTestArtifacts();

  let testFiles;
  try {
    testFiles = resolveRequestedTestFiles(args);
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }

  run("node", ["--test", ...testFiles]);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main();
}
