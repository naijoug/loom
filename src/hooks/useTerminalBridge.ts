import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import type { TerminalSlot } from "../domain";

/** List / persist the per-project terminal slots (`.loom/terminal-slots.json`). */
export function useTerminalBridge() {
  const listTerminalSlots = useCallback(async (projectPath: string) => {
    try {
      return await invokeCommand<TerminalSlot[]>(TAURI_COMMANDS.listTerminalSlots, {
        projectPath,
      });
    } catch {
      return [];
    }
  }, []);

  const saveTerminalSlots = useCallback(
    async (projectPath: string, slots: TerminalSlot[]) => {
      try {
        return await invokeCommand<TerminalSlot[]>(TAURI_COMMANDS.saveTerminalSlots, {
          projectPath,
          slots,
        });
      } catch {
        return null;
      }
    },
    [],
  );

  // Project-aware defaults: scans the repo (incl. subdir apps) for real
  // dev/test/build manifests instead of guessing pnpm at the root.
  const suggestTerminalSlots = useCallback(async (projectPath: string) => {
    try {
      return await invokeCommand<TerminalSlot[]>(TAURI_COMMANDS.suggestTerminalSlots, {
        projectPath,
      });
    } catch {
      return [];
    }
  }, []);

  return { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots };
}
