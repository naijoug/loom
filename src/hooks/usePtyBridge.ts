import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { CommandRun, CommandRunStopResult, PtySpec } from "../domain";
import { useAppState } from "../state/AppStateContext";

function toErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

/**
 * Bridge for PTY-backed terminal runs (dev-server slots in the Testing
 * cockpit). Output does not flow through `commandLogs` — it streams raw bytes
 * on `loom://pty-output`, which `TerminalCard` writes straight into xterm.js.
 * Run lifecycle (`commands/started` / `loom://command-finished`) reuses the
 * existing command-run state so the status pill and cycles keep working.
 */
export function usePtyBridge() {
  const { dispatch } = useAppState();

  const startPtyRun = useCallback(
    async (spec: PtySpec) => {
      try {
        const run = await invoke<CommandRun>("start_pty_run", { spec });
        dispatch({ type: "commands/started", run });
        return run;
      } catch (error) {
        dispatch({ type: "commands/failed", error: toErrorMessage(error) });
        return null;
      }
    },
    [dispatch],
  );

  const stopPtyRun = useCallback(
    async (runId: string) => {
      try {
        const result = await invoke<CommandRunStopResult>("stop_pty_run", { runId });
        dispatch({
          type: "commands/finished",
          event: {
            taskId: "",
            runId,
            status: "cancelled",
            exitCode: result.exitCode,
            errorSummary: undefined,
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

  const writePty = useCallback(async (runId: string, data: string) => {
    try {
      await invoke("write_pty", { runId, data });
    } catch {
      // input best-effort; a dead pty just drops keystrokes
    }
  }, []);

  const resizePty = useCallback(async (runId: string, rows: number, cols: number) => {
    try {
      await invoke("resize_pty", { runId, rows, cols });
    } catch {
      // resize best-effort
    }
  }, []);

  return { startPtyRun, stopPtyRun, writePty, resizePty };
}
