const test = require("node:test");
const assert = require("node:assert/strict");
const { appReducer, initialAppState } = require("../../.tmp/test-build/src/state/reducer.js");

function commandRun(id, taskId = "task-1") {
  return {
    id,
    taskId,
    command: id === "run-1" ? "pnpm dev" : "cargo tauri dev",
    cwd: "/tmp/project",
    startedAtMs: Date.now(),
    status: "running",
  };
}

function logEvent(runId, line, stream = "stdout") {
  return {
    runId,
    taskId: "task-1",
    stream,
    line,
    timestampMs: Date.now(),
  };
}

function planningLogEvent(line, overrides = {}) {
  return {
    taskId: "task-1",
    planningRunId: "planning-1",
    agentId: "agent-codex",
    agentName: "Codex",
    phase: "planning",
    attempt: 1,
    stream: "stdout",
    lines: [line],
    timestampMs: Date.now(),
    ...overrides,
  };
}

function taskFixture(overrides = {}) {
  return {
    id: "task-1",
    projectPath: "/tmp/project",
    title: "Task",
    rawRequirement: "Requirement",
    status: "ready_to_implement",
    selectedPlanningAgentIds: [],
    reviewAgentIds: [],
    planningRuns: [],
    agentInvocations: [],
    planReviews: [],
    planningDecisions: [],
    planTodos: [
      {
        id: "todo-1",
        taskId: "task-1",
        title: "Todo 1",
        description: "First todo",
        status: "pending",
        order: 0,
      },
    ],
    events: [],
    commandRuns: [],
    feedback: [],
    createdAtMs: 1,
    updatedAtMs: 1,
    ...overrides,
  };
}

test("command logs are bucketed by run id and starting another run does not clear existing logs", () => {
  let state = appReducer(initialAppState, { type: "commands/started", run: commandRun("run-1") });
  state = appReducer(state, { type: "commands/logReceived", event: logEvent("run-1", "vite ready") });
  state = appReducer(state, { type: "commands/started", run: commandRun("run-2") });
  state = appReducer(state, { type: "commands/logReceived", event: logEvent("run-2", "backend panic", "stderr") });

  assert.deepEqual(
    Object.keys(state.commandLogs).sort(),
    ["run-1", "run-2"],
  );
  assert.equal(state.commandLogs["run-1"].length, 1);
  assert.equal(state.commandLogs["run-1"][0].line, "vite ready");
  assert.equal(state.commandLogs["run-2"].length, 1);
  assert.equal(state.commandLogs["run-2"][0].stream, "stderr");
});

test("command log buckets are independently capped", () => {
  let state = appReducer(initialAppState, { type: "commands/started", run: commandRun("run-1") });
  state = appReducer(state, { type: "commands/started", run: commandRun("run-2") });

  for (let index = 0; index < 305; index += 1) {
    state = appReducer(state, {
      type: "commands/logReceived",
      event: logEvent("run-1", `line-${index}`),
    });
  }

  state = appReducer(state, { type: "commands/logReceived", event: logEvent("run-2", "only-line") });

  assert.equal(state.commandLogs["run-1"].length, 300);
  assert.equal(state.commandLogs["run-1"][0].line, "line-5");
  assert.equal(state.commandLogs["run-2"].length, 1);
  assert.equal(state.commandLogs["run-2"][0].line, "only-line");
});

test("selecting a task routes to task-detail and preserves a valid selected todo", () => {
  const task = taskFixture();
  let state = appReducer(initialAppState, { type: "tasks/loaded", tasks: [task] });

  state = appReducer(state, { type: "tasks/selected", taskId: task.id });

  assert.equal(state.app.currentView, "task-detail");
  assert.equal(state.app.selectedTaskId, task.id);
  assert.equal(state.app.selectedTodoId, "todo-1");
});

test("starting a new task opens the planning room with no task selected", () => {
  const task = taskFixture();
  let state = appReducer(initialAppState, { type: "tasks/loaded", tasks: [task] });
  state = appReducer(state, { type: "tasks/selected", taskId: task.id });

  state = appReducer(state, { type: "tasks/new" });

  assert.equal(state.app.currentView, "planning");
  assert.equal(state.app.selectedTaskId, null);
  assert.equal(state.app.selectedTodoId, null);
});

test("removing the selected task drops it and returns to the board", () => {
  const task = taskFixture();
  let state = appReducer(initialAppState, { type: "tasks/loaded", tasks: [task] });
  state = appReducer(state, { type: "tasks/selected", taskId: task.id });

  state = appReducer(state, { type: "tasks/removed", taskId: task.id });

  assert.equal(state.tasks.length, 0);
  assert.equal(state.app.selectedTaskId, null);
  assert.equal(state.app.currentView, "board");
});

test("removing an unselected task keeps the current selection and view", () => {
  const selected = taskFixture({ id: "task-keep" });
  const other = taskFixture({ id: "task-drop" });
  let state = appReducer(initialAppState, { type: "tasks/loaded", tasks: [selected, other] });
  state = appReducer(state, { type: "tasks/selected", taskId: "task-keep" });

  state = appReducer(state, { type: "tasks/removed", taskId: "task-drop" });

  assert.equal(state.tasks.length, 1);
  assert.equal(state.tasks[0].id, "task-keep");
  assert.equal(state.app.selectedTaskId, "task-keep");
  assert.equal(state.app.currentView, "task-detail");
});

test("completing all todos moves the optimistic task state to reviewing", () => {
  const task = taskFixture({
    planTodos: [
      {
        id: "todo-1",
        taskId: "task-1",
        title: "Todo 1",
        description: "First todo",
        status: "implementing",
        order: 0,
      },
    ],
  });
  let state = appReducer(initialAppState, { type: "tasks/loaded", tasks: [task] });

  state = appReducer(state, { type: "tasks/todoCompleted", taskId: task.id, todoId: "todo-1" });

  assert.equal(state.tasks[0].status, "reviewing");
  assert.equal(state.tasks[0].planTodos[0].status, "done");
});

test("planning progress replaces queued entries with backend status events", () => {
  let state = appReducer(initialAppState, {
    type: "planning/progressQueued",
    taskId: "task-1",
    agents: [
      { id: "agent-codex", name: "Codex" },
      { id: "agent-claude", name: "Claude Code" },
    ],
  });

  assert.equal(Object.keys(state.planningProgress).length, 2);
  assert.equal(state.planningProgress["pending:planning:agent-codex"].status, "pending");

  state = appReducer(state, {
    type: "planning/progressUpdated",
    event: {
      taskId: "task-1",
      planningRunId: "planning-1",
      agentId: "agent-codex",
      agentName: "Codex",
      phase: "planning",
      status: "running",
      startedAtMs: 100,
    },
  });

  assert.equal(state.planningProgress["pending:planning:agent-codex"], undefined);
  assert.equal(state.planningProgress["pending:planning:agent-claude"], undefined);
  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].status, "running");

  state = appReducer(state, {
    type: "planning/progressUpdated",
    event: {
      taskId: "task-1",
      planningRunId: "planning-1",
      agentId: "agent-codex",
      agentName: "Codex",
      phase: "planning",
      status: "succeeded",
      startedAtMs: 100,
      endedAtMs: 180,
      elapsedMs: 80,
    },
  });

  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].status, "succeeded");
  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].elapsedMs, 80);
});

test("planning progress tracks retry attempts and review pairs separately", () => {
  let state = appReducer(initialAppState, {
    type: "planning/progressUpdated",
    event: {
      taskId: "task-1",
      planningRunId: "planning-1",
      agentId: "agent-codex",
      agentName: "Codex",
      phase: "planning",
      status: "retrying",
      attempt: 1,
      startedAtMs: 100,
      endedAtMs: 150,
      elapsedMs: 50,
    },
  });

  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].status, "retrying");

  state = appReducer(state, {
    type: "planning/progressUpdated",
    event: {
      taskId: "task-1",
      planningRunId: "planning-1",
      agentId: "agent-codex",
      agentName: "Codex",
      phase: "planning",
      status: "succeeded",
      attempt: 2,
      startedAtMs: 160,
      endedAtMs: 220,
      elapsedMs: 60,
    },
  });

  // The retry overwrites the same agent/phase slot instead of duplicating it.
  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].status, "succeeded");
  assert.equal(state.planningProgress["planning-1:planning:agent-codex"].attempt, 2);

  state = appReducer(state, {
    type: "planning/progressUpdated",
    event: {
      taskId: "task-1",
      planningRunId: "planning-1",
      agentId: "agent-codex->agent-claude",
      agentName: "Codex → Claude Code",
      phase: "review",
      status: "running",
      attempt: 1,
      startedAtMs: 230,
    },
  });

  assert.equal(Object.keys(state.planningProgress).length, 2);
  assert.equal(
    state.planningProgress["planning-1:review:agent-codex->agent-claude"].phase,
    "review",
  );
});

test("planning logs are bucketed by run phase agent and capped", () => {
  let state = initialAppState;

  for (let index = 0; index < 305; index += 1) {
    state = appReducer(state, {
      type: "planning/logReceived",
      event: planningLogEvent(`line-${index}`),
    });
  }
  state = appReducer(state, {
    type: "planning/logReceived",
    event: planningLogEvent("review-line", {
      phase: "review",
      agentId: "agent-codex->agent-claude",
      agentName: "Codex → Claude",
    }),
  });

  assert.equal(state.planningLogs["planning-1:planning:agent-codex"].length, 300);
  assert.equal(state.planningLogs["planning-1:planning:agent-codex"][0].lines[0], "line-5");
  assert.equal(
    state.planningLogs["planning-1:review:agent-codex->agent-claude"][0].lines[0],
    "review-line",
  );
});

test("planning progress queue clears prior planning logs for the same task", () => {
  let state = appReducer(initialAppState, {
    type: "planning/logReceived",
    event: planningLogEvent("old-line"),
  });
  state = appReducer(state, {
    type: "planning/logReceived",
    event: planningLogEvent("other-task-line", { taskId: "task-2", planningRunId: "planning-2" }),
  });

  state = appReducer(state, {
    type: "planning/progressQueued",
    taskId: "task-1",
    agents: [{ id: "agent-codex", name: "Codex" }],
  });

  assert.equal(state.planningLogs["planning-1:planning:agent-codex"], undefined);
  assert.equal(Object.values(state.planningLogs).flat()[0].taskId, "task-2");
});
