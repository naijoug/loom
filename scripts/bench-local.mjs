import { spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const testBuild = join(root, ".tmp", "test-build");
const budgets = {
  previewColdStartMs: 5000,
  projectScanMs: 250,
  logIngest10kMs: 500,
  timelineModel5kMs: 200,
  timelineRender1kMs: 500,
  rssMb: 256,
};

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: root,
    encoding: "utf8",
    ...options,
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed\n${result.stdout ?? ""}\n${result.stderr ?? ""}`);
  }
  return `${result.stdout ?? ""}\n${result.stderr ?? ""}`;
}

function measure(operation) {
  const started = performance.now();
  const value = operation();
  return { value, durationMs: performance.now() - started };
}

async function previewColdStart() {
  run("./scripts/preview.sh", ["stop"]);
  const started = performance.now();
  try {
    run("./scripts/preview.sh", ["start"]);
    const response = await fetch("http://127.0.0.1:1420/preview/planning?screen=planning");
    if (!response.ok) throw new Error(`preview returned HTTP ${response.status}`);
    await response.arrayBuffer();
    return performance.now() - started;
  } finally {
    run("./scripts/preview.sh", ["stop"]);
  }
}

run("pnpm", ["exec", "tsc", "-p", "tsconfig.test.json"]);
mkdirSync(testBuild, { recursive: true });
writeFileSync(join(testBuild, "package.json"), '{"type":"commonjs"}\n');

const require = createRequire(import.meta.url);
const React = require("react");
const { renderToStaticMarkup } = require("react-dom/server");
const { appReducer, initialAppState } = require(join(testBuild, "src/state/reducer.js"));
const { buildTaskTimelineRows } = require(join(testBuild, "src/utils/taskTimeline.js"));
const { TaskTimeline } = require(join(testBuild, "src/components/TaskDetail/TaskTimeline.js"));

const projectProbe = run("cargo", [
  "test",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "projects::tests::benchmark_project_analysis",
  "--",
  "--ignored",
  "--nocapture",
]);
const projectScanMatch = projectProbe.match(/LOOM_PROJECT_SCAN_MS=([0-9.]+)/);
if (!projectScanMatch) throw new Error("project scan probe did not report a measurement");

const logIngest = measure(() => {
  let state = initialAppState;
  for (let index = 0; index < 10_000; index += 1) {
    state = appReducer(state, {
      type: "commands/logReceived",
      event: {
        taskId: "bench-task",
        runId: "bench-run",
        stream: index % 10 === 0 ? "stderr" : "stdout",
        line: `line-${index}`,
        timestampMs: index + 1,
      },
    });
  }
  if (state.commandLogs["bench-run"].length !== 300) {
    throw new Error("log reducer cap changed during benchmark");
  }
});

const commandRuns = Array.from({ length: 5_000 }, (_, index) => ({
  id: `run-${index}`,
  taskId: "bench-task",
  command: `pnpm test --filter case-${index}`,
  cwd: root,
  intent: "validation",
  startedAtMs: index + 1,
  endedAtMs: index + 2,
  status: index % 7 === 0 ? "failed" : "succeeded",
  exitCode: index % 7 === 0 ? 1 : 0,
}));
const longTask = {
  id: "bench-task",
  projectPath: root,
  title: "Performance fixture",
  rawRequirement: "Measure a long task",
  status: "debugging",
  lifecycle: { paused: false },
  selectedPlanningAgentIds: [],
  reviewAgentIds: [],
  implementationReviewRuns: [],
  implementationReviews: [],
  implementationReviewDecisions: [],
  planningRuns: [],
  agentInvocations: [],
  planReviews: [],
  planningDecisions: [],
  planTodos: [],
  loopTrace: [],
  events: [],
  commandRuns,
  feedback: [],
  createdAtMs: 1,
  updatedAtMs: 5_001,
};
const rssBefore = process.memoryUsage().rss;
const timelineModel = measure(() => buildTaskTimelineRows(longTask, 5_000));
if (timelineModel.value.length !== 5_000) throw new Error("timeline model dropped benchmark rows");
const timelineRender = measure(() => renderToStaticMarkup(
  React.createElement(TaskTimeline, { task: longTask, maxItems: 1_000 }),
));
if (!timelineRender.value.includes("task-timeline")) throw new Error("timeline did not render");

const metrics = {
  previewColdStartMs: Number((await previewColdStart()).toFixed(2)),
  projectScanMs: Number(Number(projectScanMatch[1]).toFixed(2)),
  logIngest10kMs: Number(logIngest.durationMs.toFixed(2)),
  timelineModel5kMs: Number(timelineModel.durationMs.toFixed(2)),
  timelineRender1kMs: Number(timelineRender.durationMs.toFixed(2)),
  rssMb: Number((process.memoryUsage().rss / 1024 / 1024).toFixed(2)),
  rssDeltaMb: Number(((process.memoryUsage().rss - rssBefore) / 1024 / 1024).toFixed(2)),
};
const failures = Object.entries(budgets)
  .filter(([name, limit]) => metrics[name] > limit)
  .map(([name, limit]) => `${name}: ${metrics[name]} > ${limit}`);

console.log(JSON.stringify({ metrics, budgets, failures }, null, 2));
console.log("\n| 指标 | 实测 | 预算 | 结果 |");
console.log("|---|---:|---:|---|");
for (const [name, limit] of Object.entries(budgets)) {
  console.log(`| ${name} | ${metrics[name]} | ${limit} | ${metrics[name] <= limit ? "通过" : "超限"} |`);
}
if (failures.length > 0) {
  console.error(`Performance budgets failed:\n- ${failures.join("\n- ")}`);
  process.exit(1);
}
