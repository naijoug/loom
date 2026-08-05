import { useCallback, useEffect } from "react";
import { invokeCommand, listenToEvent, TAURI_COMMANDS, TAURI_EVENTS } from "../api";
import type {
  CommandFinishedEvent,
  CommandLogEvent,
  CommandRun,
  CommandRunStopResult,
  CommandSpec,
} from "../domain";
import { useAppState } from "../state/AppStateContext";
import { authorizeExecution } from "../utils/executionPolicy";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function useCommandBridge() {
  const { dispatch } = useAppState();

  useEffect(() => {
    const unlisten = listenToEvent<CommandLogEvent>(TAURI_EVENTS.commandLog, (event) => {
      dispatch({ type: "commands/logReceived", event });
    });
    const unlistenFinished = listenToEvent<CommandFinishedEvent>(
      TAURI_EVENTS.commandFinished,
      (event) => {
        dispatch({ type: "commands/finished", event });
      },
    );

    return () => {
      void unlisten.then((remove) => remove());
      void unlistenFinished.then((remove) => remove());
    };
  }, [dispatch]);

  const startCommandRun = useCallback(
    async (spec: CommandSpec) => {
      try {
        const approvalId = await authorizeExecution(spec);
        const run = await invokeCommand<CommandRun>(TAURI_COMMANDS.startCommandRun, {
          spec: { ...spec, approvalId },
        });
        dispatch({ type: "commands/started", run });
        return run;
      } catch (error) {
        dispatch({ type: "commands/failed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const stopCommandRun = useCallback(
    async (runId: string, terminationReason?: string) => {
      try {
        const result = await invokeCommand<CommandRunStopResult>(TAURI_COMMANDS.stopCommandRun, {
          runId,
          terminationReason,
        });
        dispatch({
          type: "commands/finished",
          event: {
            taskId: "",
            runId,
            status: "cancelled",
            exitCode: result.exitCode,
            errorSummary: undefined,
            terminationReason: terminationReason ?? "cancelled",
            timestampMs: Date.now(),
          },
        });
        return result;
      } catch (error) {
        dispatch({ type: "commands/failed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const loadCommandRunLogs = useCallback(
    async (projectPath: string, taskId: string, runId: string) => {
      try {
        const events = await invokeCommand<CommandLogEvent[]>(TAURI_COMMANDS.readCommandRunLogs, {
          projectPath,
          taskId,
          runId,
        });
        dispatch({ type: "commands/logHistoryLoaded", runId, events });
        return events;
      } catch (error) {
        dispatch({ type: "commands/failed", error: toErrorMessage(error) });
        return [];
      }
    },
    [dispatch],
  );

  return { startCommandRun, stopCommandRun, loadCommandRunLogs };
}
