import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, join, relative, resolve } from "node:path";
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

if (existsSync(outDir)) {
  rmSync(outDir, { recursive: true, force: true });
}

mkdirSync(outDir, { recursive: true });
run("pnpm", ["exec", "tsc", "-p", "tsconfig.test.json"]);
writeFileSync(join(outDir, "package.json"), '{"type":"commonjs"}\n');

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

writeStyleStubs(join(root, "src"));

const requestedTestFiles = process.argv.slice(2).filter((arg) => arg.endsWith(".test.cjs"));
const testDir = join(root, "tests", "unit");
const testFiles = requestedTestFiles.length > 0
  ? requestedTestFiles.map((file) => (isAbsolute(file) ? file : resolve(root, file)))
  : readdirSync(testDir)
      .filter((file) => file.endsWith(".test.cjs"))
      .sort()
      .map((file) => join(testDir, file));

run("node", ["--test", ...testFiles]);
