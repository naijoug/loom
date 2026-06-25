import { invoke } from "@tauri-apps/api/core";
import { useCallback } from "react";
import type { TerminalSlot } from "../domain";

/** List / persist the per-project terminal slots (`.loom/terminal-slots.json`). */
export function useTerminalBridge() {
  const listTerminalSlots = useCallback(async (projectPath: string) => {
    try {
      return await invoke<TerminalSlot[]>("list_terminal_slots", { projectPath });
    } catch {
      return [];
    }
  }, []);

  const saveTerminalSlots = useCallback(
    async (projectPath: string, slots: TerminalSlot[]) => {
      try {
        return await invoke<TerminalSlot[]>("save_terminal_slots", { projectPath, slots });
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
      return await invoke<TerminalSlot[]>("suggest_terminal_slots", { projectPath });
    } catch {
      return [];
    }
  }, []);

  return { listTerminalSlots, saveTerminalSlots, suggestTerminalSlots };
}
