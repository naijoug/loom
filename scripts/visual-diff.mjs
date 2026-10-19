import { readFileSync, readdirSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { gunzipSync, gzipSync } from "node:zlib";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";

const GRID_WIDTH = 24;
const GRID_HEIGHT = 16;
const MEAN_DIFF_LIMIT = Number(process.env.LOOM_VISUAL_MEAN_DIFF_LIMIT ?? "0.012");
const CHANGED_CELL_LIMIT = Number(process.env.LOOM_VISUAL_CHANGED_CELL_LIMIT ?? "0.08");
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
          rgba: sampleImage(png),
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
  const labels = Object.keys(currentSamples);
  const dimensions = [...new Set(Object.values(currentSamples).map((sample) => `${sample.width}x${sample.height}`))];
  if (dimensions.length !== 1) throw new Error(`screenshots have inconsistent dimensions: ${dimensions.join(", ")}`);
  const [width, height] = dimensions[0].split("x").map(Number);
  const rgbaGzip = gzipSync(
    Buffer.concat(labels.map((label) => currentSamples[label].rgba)),
    { level: 9 },
  ).toString("base64");
  process.stdout.write(`${JSON.stringify({ version: 3, grid: [GRID_WIDTH, GRID_HEIGHT], dimensions: [width, height], labels, rgbaGzip }, null, 2)}\n`);
  process.exit(0);
}

const baseline = JSON.parse(readFileSync(baselinePath, "utf8"));
const failures = [];
const sampleBytes = GRID_WIDTH * GRID_HEIGHT * 4;
let baselinePixels = Buffer.alloc(0);
if (baseline.version !== 3 || baseline.grid?.[0] !== GRID_WIDTH || baseline.grid?.[1] !== GRID_HEIGHT) {
  failures.push(`baseline format mismatch: expected version 3 and ${GRID_WIDTH}x${GRID_HEIGHT} grid`);
} else {
  try {
    baselinePixels = gunzipSync(Buffer.from(baseline.rgbaGzip, "base64"));
    if (baselinePixels.length !== baseline.labels.length * sampleBytes) {
      failures.push(`invalid baseline payload length ${baselinePixels.length}`);
    }
  } catch (error) {
    failures.push(`invalid compressed baseline (${error instanceof Error ? error.message : error})`);
  }
}
for (const [label, currentMeta] of Object.entries(currentSamples)) {
  const baselineIndex = baseline.labels?.indexOf(label) ?? -1;
  if (baselineIndex < 0) {
    failures.push(`${label}: baseline missing`);
    continue;
  }
  if (baseline.dimensions?.[0] !== currentMeta.width || baseline.dimensions?.[1] !== currentMeta.height) {
    failures.push(`${label}: dimensions changed ${baseline.dimensions?.join("x")} -> ${currentMeta.width}x${currentMeta.height}`);
    continue;
  }
  const start = baselineIndex * sampleBytes;
  const expected = baselinePixels.subarray(start, start + sampleBytes);
  const current = currentMeta.rgba;
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
    if (cellDiff > 0.04) changedCells += 1;
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

for (const label of baseline.labels ?? []) {
  if (!currentSamples[label]) failures.push(`${label}: screenshot missing`);
}

if (failures.length > 0) {
  console.error(`Visual baseline failed:\n${failures.map((failure) => `- ${failure}`).join("\n")}`);
  process.exit(1);
}
console.log("Visual baseline diff passed");
