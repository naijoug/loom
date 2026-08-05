import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const DEFAULT_TASK = ".loom/tasks/task-loop-timeline-canary.json";
const DEFAULT_REPORT = "docs/dogfood/2026-06-25-loop-timeline-canary.md";
const REQUIRED_VALIDATION_COMMANDS = [
  "pnpm test",
  "pnpm build",
  "cargo test --manifest-path src-tauri/Cargo.toml",
];

function parseArgs(argv) {
  const options = {
    task: DEFAULT_TASK,
    report: DEFAULT_REPORT,
    strict: false,
    headless: false,
    validationCommands: [...REQUIRED_VALIDATION_COMMANDS],
    hasCustomValidationCommands: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--") {
      continue;
    } else if (arg === "--strict") {
      options.strict = true;
    } else if (arg === "--headless") {
      options.headless = true;
    } else if (arg === "--task") {
      options.task = readArgValue(argv, index, arg);
      index += 1;
    } else if (arg === "--report") {
      options.report = readArgValue(argv, index, arg);
      index += 1;
    } else if (arg === "--validation") {
      if (!options.hasCustomValidationCommands) {
        options.validationCommands = [];
        options.hasCustomValidationCommands = true;
      }
      options.validationCommands.push(readArgValue(argv, index, arg));
      index += 1;
    } else if (arg === "-h" || arg === "--help") {
      printHelp();
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }

  return options;
}

function readArgValue(argv, index, arg) {
  const value = argv[index + 1];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`Missing value for ${arg}`);
  }
  return value;
}

function printHelp() {
  console.log(`Usage: node scripts/verify-dogfood.mjs [--strict] [--headless] [--task PATH] [--report PATH] [--validation COMMAND]

Verifies that a local Loom-on-Loom dogfood task record contains auditable
validation evidence. Default mode accepts the current record/replay canary.
Strict mode additionally requires real agent_action, failed-validation, and
repair-loop evidence. Desktop UI replay is required unless --headless is set.
Pass --validation multiple times to verify a non-default command set.`);
}

function readJson(path) {
  const document = JSON.parse(readFileSync(path, "utf8"));
  return document && typeof document === "object" && "schemaVersion" in document && "data" in document
    ? document.data
    : document;
}

function check(results, condition, message, { warn = false } = {}) {
  results.push({
    level: condition ? "pass" : warn ? "warn" : "fail",
    message,
  });
}

function commandRunFor(task, command) {
  return (task.commandRuns ?? []).find(
    (run) =>
      run.command === command &&
      run.intent === "validation" &&
      run.status === "succeeded" &&
      run.exitCode === 0,
  );
}

function traceFor(task, runId) {
  return (task.loopTrace ?? []).find(
    (entry) =>
      entry.commandRunId === runId &&
      entry.entryType === "command_finished" &&
      entry.stage === "testing",
  );
}

function desktopReplayEvent(task) {
  return (task.events ?? []).find(
    (event) =>
      typeof event.evidenceRef === "string" &&
      event.evidenceRef.includes(".loom/screenshots/") &&
      `${event.inputSummary ?? ""} ${event.outputSummary ?? ""}`.includes("Desktop UI"),
  );
}

function hasFailedValidation(task) {
  return (task.commandRuns ?? []).some(
    (run) =>
      run.intent === "validation" &&
      (run.status === "failed" || (typeof run.exitCode === "number" && run.exitCode !== 0)),
  );
}

function hasRepairLoopEvidence(task) {
  return (
    (task.commandRuns ?? []).some(
      (run) => run.intent === "agent_action" && typeof run.attempt === "number" && run.attempt > 0,
    ) ||
    (task.loopTrace ?? []).some(
      (entry) =>
        entry.stage === "implement" &&
        typeof entry.attempt === "number" &&
        entry.attempt > 0,
    ) ||
    (task.events ?? []).some((event) =>
      `${event.inputSummary ?? ""} ${event.outputSummary ?? ""}`.toLowerCase().includes("repair"),
    )
  );
}

function verify(options) {
  const taskPath = resolve(options.task);
  const reportPath = resolve(options.report);
  const results = [];

  check(results, existsSync(taskPath), `task record exists: ${options.task}`);
  check(results, existsSync(reportPath), `dogfood report exists: ${options.report}`);
  if (!existsSync(taskPath) || !existsSync(reportPath)) {
    return results;
  }

  const task = readJson(taskPath);
  const report = readFileSync(reportPath, "utf8");
  const uiReplay = desktopReplayEvent(task);

  check(results, task.status === "completed", "task status is completed");
  check(results, (task.planTodos ?? []).some((todo) => todo.status === "done"), "at least one implementation todo is done");
  check(results, (task.events ?? []).some((event) => event.status === "completed"), "completion event is recorded");
  check(results, Array.isArray(task.commandRuns) && task.commandRuns.length > 0, "command runs are persisted");
  check(results, Array.isArray(task.loopTrace) && task.loopTrace.length > 0, "loop trace is persisted");

  for (const command of options.validationCommands) {
    const run = commandRunFor(task, command);
    check(results, Boolean(run), `validation command passed: ${command}`);
    if (run) {
      check(results, Boolean(run.loopId), `validation command has loop_id: ${command}`);
      check(results, Boolean(traceFor(task, run.id)), `validation command has finished trace: ${command}`);
    }
    check(results, report.includes(command), `report references validation command: ${command}`);
  }

  const agentActionRun = (task.commandRuns ?? []).find(
    (run) => run.intent === "agent_action" && run.status === "succeeded",
  );
  const desktopUiWarnOnly = !options.strict || options.headless;
  check(results, Boolean(agentActionRun), "real agent_action command run is persisted", {
    warn: !options.strict,
  });
  check(results, Boolean(uiReplay), "desktop UI replay event is persisted", {
    warn: desktopUiWarnOnly,
  });
  if (uiReplay?.evidenceRef) {
    check(results, existsSync(resolve(uiReplay.evidenceRef)), "desktop UI replay screenshot exists", {
      warn: desktopUiWarnOnly,
    });
  }

  if (options.strict) {
    check(results, hasFailedValidation(task), "strict dogfood includes a failed validation signal");
    check(results, hasRepairLoopEvidence(task), "strict dogfood includes repair-loop evidence");
    check(results, !report.includes("record/replay canary"), "strict report is not labeled record/replay only");
    if (options.headless) {
      check(results, report.toLowerCase().includes("headless"), "headless strict report declares headless scope");
    } else {
      check(results, !report.includes("未验证项"), "strict report has no explicit unverified items");
    }
  } else {
    check(results, report.includes("record/replay canary"), "report honestly labels the current artifact as record/replay canary", {
      warn: true,
    });
  }

  return results;
}

function printResults(results) {
  for (const result of results) {
    const label = result.level.toUpperCase().padEnd(4, " ");
    console.log(`${label} ${result.message}`);
  }
}

try {
  const options = parseArgs(process.argv.slice(2));
  const results = verify(options);
  printResults(results);

  const failures = results.filter((result) => result.level === "fail");
  const warnings = results.filter((result) => result.level === "warn");
  console.log(
    `\nDogfood verification: ${failures.length} failed, ${warnings.length} warning${warnings.length === 1 ? "" : "s"}`,
  );

  if (failures.length > 0) {
    process.exit(1);
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
