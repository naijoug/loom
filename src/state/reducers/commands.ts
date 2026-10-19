import type { AppAction, AppState } from "../model";
import { syncCommandRunFinished, syncCommandRunStarted } from "./commandTrace";
import { mapCachedTasks } from "./taskCollections";

export function reduceCommands(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "commands/started": {
      const tasks = syncCommandRunStarted(state.tasks, action.run);
      return {
        ...state,
        app: { ...state.app, activeCommandRunId: action.run.id, commandError: null },
        tasks,
        taskCache: mapCachedTasks(state.taskCache, action.run.taskId, (task) =>
          syncCommandRunStarted([task], action.run)[0] ?? task,
        ),
        commandRuns: [...state.commandRuns.filter((run) => run.id !== action.run.id), action.run],
        commandLogs: {
          ...state.commandLogs,
          [action.run.id]: state.commandLogs[action.run.id] ?? [],
        },
      };
    }
    case "commands/logReceived": {
      const runLogs = state.commandLogs[action.event.runId] ?? [];
      return {
        ...state,
        commandLogs: {
          ...state.commandLogs,
          [action.event.runId]: [...runLogs.slice(-299), action.event],
        },
      };
    }
    case "commands/logHistoryLoaded":
      if ((state.commandLogs[action.runId]?.length ?? 0) > 0) return state;
      return {
        ...state,
        commandLogs: { ...state.commandLogs, [action.runId]: action.events },
      };
    case "commands/finished": {
      const finishedRun = state.commandRuns.find((run) => run.id === action.event.runId);
      const updatedRun = finishedRun
        ? {
            ...finishedRun,
            status: action.event.status,
            exitCode: action.event.exitCode,
            errorSummary: action.event.errorSummary,
            sessionId: action.event.sessionId ?? finishedRun.sessionId,
            resumeCommand: action.event.resumeCommand ?? finishedRun.resumeCommand,
            terminationReason:
              action.event.terminationReason ??
              finishedRun.terminationReason ??
              (action.event.status === "cancelled" ? "cancelled" : undefined),
            endedAtMs: action.event.timestampMs,
          }
        : null;
      const tasks = updatedRun ? syncCommandRunFinished(state.tasks, updatedRun) : state.tasks;
      return {
        ...state,
        app: {
          ...state.app,
          activeCommandRunId:
            state.app.activeCommandRunId === action.event.runId ? null : state.app.activeCommandRunId,
        },
        tasks,
        taskCache: updatedRun
          ? mapCachedTasks(state.taskCache, updatedRun.taskId, (task) =>
              syncCommandRunFinished([task], updatedRun)[0] ?? task,
            )
          : state.taskCache,
        commandRuns: state.commandRuns.map((run) =>
          run.id === action.event.runId && updatedRun ? updatedRun : run,
        ),
      };
    }
    case "commands/failed":
      return { ...state, app: { ...state.app, commandError: action.error } };
    case "commands/cleared":
      return { ...state, commandLogs: {} };
    default:
      return state;
  }
}
