import { useEffect } from "react";
import { listenToEvent, TAURI_EVENTS } from "../api";
import type { PlanningAgentLogEvent, PlanningAgentStatusEvent } from "../domain";
import { useAppState } from "../state/AppStateContext";
import { hasTauriRuntime } from "./runtime";

/** Mounted once by the advanced workflow, never by Chat or Settings. */
export function usePlanningEvents() {
  const { dispatch } = useAppState();
  useEffect(() => {
    if (!hasTauriRuntime()) return;
    let disposed = false;
    const cleanups: Array<() => void> = [];
    const keep = (cleanup: () => void) => disposed ? cleanup() : cleanups.push(cleanup);
    const failed = (error: unknown) => {
      if (!disposed) dispatch({ type: "tasks/loadFailed", error: String(error) });
    };
    void listenToEvent<PlanningAgentStatusEvent>(TAURI_EVENTS.planningAgentStatus, (event) => {
      if (!disposed) dispatch({ type: "planning/progressUpdated", event });
    }).then(keep).catch(failed);
    void listenToEvent<PlanningAgentLogEvent>(TAURI_EVENTS.planningAgentLog, (event) => {
      if (!disposed) dispatch({ type: "planning/logReceived", event });
    }).then(keep).catch(failed);
    return () => { disposed = true; cleanups.forEach((cleanup) => cleanup()); };
  }, [dispatch]);
}
