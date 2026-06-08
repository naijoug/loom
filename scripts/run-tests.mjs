import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
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

const testDir = join(root, "tests", "unit");
const testFiles = readdirSync(testDir)
  .filter((file) => file.endsWith(".test.cjs"))
  .map((file) => join(testDir, file));

run("node", ["--test", ...testFiles]);
