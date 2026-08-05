import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect } from "react";
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
    const unlisten = listen<CommandLogEvent>("loom://command-log", (event) => {
      dispatch({ type: "commands/logReceived", event: event.payload });
    });
    const unlistenFinished = listen<CommandFinishedEvent>("loom://command-finished", (event) => {
      dispatch({ type: "commands/finished", event: event.payload });
    });

    return () => {
      void unlisten.then((remove) => remove());
      void unlistenFinished.then((remove) => remove());
    };
  }, [dispatch]);

  const startCommandRun = useCallback(
    async (spec: CommandSpec) => {
      try {
        const approvalId = await authorizeExecution(spec);
        const run = await invoke<CommandRun>("start_command_run", {
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
        const result = await invoke<CommandRunStopResult>("stop_command_run", {
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
        const events = await invoke<CommandLogEvent[]>("read_command_run_logs", {
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
