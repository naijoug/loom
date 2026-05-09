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
      dispatch({ type: "commands/cleared" });
      try {
        const run = await invoke<CommandRun>("start_command_run", { spec });
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
    async (runId: string) => {
      try {
        const result = await invoke<CommandRunStopResult>("stop_command_run", { runId });
        dispatch({
          type: "commands/finished",
          event: {
            taskId: "",
            runId,
            status: "cancelled",
            exitCode: result.exitCode,
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

  return { startCommandRun, stopCommandRun };
}
