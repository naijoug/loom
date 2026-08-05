import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const GRID_WIDTH = 8;
const GRID_HEIGHT = 6;
const MEAN_DIFF_LIMIT = Number(process.env.LOOM_VISUAL_MEAN_DIFF_LIMIT ?? "0.035");
const CHANGED_CELL_LIMIT = Number(process.env.LOOM_VISUAL_CHANGED_CELL_LIMIT ?? "0.18");
const root = new URL("..", import.meta.url).pathname;
const baselinePath = join(root, "tests", "visual", "baselines.json");

function sampleImage(png) {
  const sampled = Buffer.alloc(GRID_WIDTH * GRID_HEIGHT * 4);
  for (let gridY = 0; gridY < GRID_HEIGHT; gridY += 1) {
    const fromY = Math.floor((gridY * png.height) / GRID_HEIGHT);
    const toY = Math.max(fromY + 1, Math.floor(((gridY + 1) * png.height) / GRID_HEIGHT));
    for (let gridX = 0; gridX < GRID_WIDTH; gridX += 1) {
      const fromX = Math.floor((gridX * png.width) / GRID_WIDTH);
      const toX = Math.max(fromX + 1, Math.floor(((gridX + 1) * png.width) / GRID_WIDTH));
      const totals = [0, 0, 0, 0];
      let count = 0;
      for (let y = fromY; y < toY; y += 1) {
        for (let x = fromX; x < toX; x += 1) {
          const offset = (y * png.width + x) * 4;
          for (let channel = 0; channel < 4; channel += 1) totals[channel] += png.data[offset + channel];
          count += 1;
        }
      }
      const target = (gridY * GRID_WIDTH + gridX) * 4;
      for (let channel = 0; channel < 4; channel += 1) sampled[target + channel] = Math.round(totals[channel] / count);
    }
  }
  return sampled;
}

function readSamples(directory) {
  return Object.fromEntries(
    readdirSync(directory)
      .filter((file) => file.endsWith(".png") && !file.endsWith(".diff.png"))
      .sort()
      .map((file) => {
        const png = PNG.sync.read(readFileSync(join(directory, file)));
        return [basename(file, ".png"), {
          width: png.width,
          height: png.height,
          rgba: sampleImage(png).toString("base64"),
        }];
      }),
  );
}

function writeDiff(directory, label, baseline, current) {
  const small = new PNG({ width: GRID_WIDTH, height: GRID_HEIGHT });
  pixelmatch(baseline, current, small.data, GRID_WIDTH, GRID_HEIGHT, { threshold: 0.1 });
  const scale = 20;
  const expanded = new PNG({ width: GRID_WIDTH * scale, height: GRID_HEIGHT * scale });
  for (let y = 0; y < expanded.height; y += 1) {
    for (let x = 0; x < expanded.width; x += 1) {
      const source = (Math.floor(y / scale) * GRID_WIDTH + Math.floor(x / scale)) * 4;
      const target = (y * expanded.width + x) * 4;
      small.data.copy(expanded.data, target, source, source + 4);
    }
  }
  const target = join(directory, `${label}.diff.png`);
  writeFileSync(target, PNG.sync.write(expanded));
  return target;
}

const [mode, directory] = process.argv.slice(2);
if (!directory) {
  throw new Error("usage: node scripts/visual-diff.mjs [--emit-baseline|--check] <screenshot-dir>");
}

const currentSamples = readSamples(directory);
if (mode === "--emit-baseline") {
  process.stdout.write(`${JSON.stringify({ version: 1, grid: [GRID_WIDTH, GRID_HEIGHT], samples: currentSamples }, null, 2)}\n`);
  process.exit(0);
}

const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const failures = [];
if (baseline.version !== 1 || baseline.grid?.[0] !== GRID_WIDTH || baseline.grid?.[1] !== GRID_HEIGHT) {
  failures.push(`baseline format mismatch: expected version 1 and ${GRID_WIDTH}x${GRID_HEIGHT} grid`);
}
for (const [label, currentMeta] of Object.entries(currentSamples)) {
  const baselineMeta = baseline.samples[label];
  if (!baselineMeta) {
    failures.push(`${label}: baseline missing`);
    continue;
  }
  if (baselineMeta.width !== currentMeta.width || baselineMeta.height !== currentMeta.height) {
    failures.push(`${label}: dimensions changed ${baselineMeta.width}x${baselineMeta.height} -> ${currentMeta.width}x${currentMeta.height}`);
    continue;
  }
  const expected = Buffer.from(baselineMeta.rgba, "base64");
  const current = Buffer.from(currentMeta.rgba, "base64");
  if (expected.length !== current.length) {
    failures.push(`${label}: invalid baseline sample length ${expected.length}, expected ${current.length}`);
    continue;
  }
  let total = 0;
  let changedCells = 0;
  for (let offset = 0; offset < current.length; offset += 4) {
    const cellDiff = (
      Math.abs(expected[offset] - current[offset]) +
      Math.abs(expected[offset + 1] - current[offset + 1]) +
      Math.abs(expected[offset + 2] - current[offset + 2])
    ) / (3 * 255);
    total += cellDiff;
    if (cellDiff > 0.06) changedCells += 1;
  }
  const cells = GRID_WIDTH * GRID_HEIGHT;
  const meanDiff = total / cells;
  const changedRatio = changedCells / cells;
  if (meanDiff > MEAN_DIFF_LIMIT || changedRatio > CHANGED_CELL_LIMIT) {
    const artifact = writeDiff(directory, label, expected, current);
    failures.push(`${label}: mean=${meanDiff.toFixed(4)}, changed=${(changedRatio * 100).toFixed(1)}%, diff=${artifact}`);
  } else {
    console.log(`DIFF ${label}: mean=${meanDiff.toFixed(4)}, changed=${(changedRatio * 100).toFixed(1)}%`);
  }
}

for (const label of Object.keys(baseline.samples)) {
  if (!currentSamples[label]) failures.push(`${label}: screenshot missing`);
}

if (failures.length > 0) {
  console.error(`Visual baseline failed:\n${failures.map((failure) => `- ${failure}`).join("\n")}`);
  process.exit(1);
}
console.log("Visual baseline diff passed");
