import type { PlanningAgentLogEvent, PlanningAgentStatusEvent } from "../../domain";
import type { AppAction, AppState } from "../model";

function planningProgressKey(event: PlanningAgentStatusEvent) {
  return `${event.planningRunId}:${event.phase}:${event.agentId}`;
}

export function planningLogKey(
  event: Pick<PlanningAgentLogEvent, "planningRunId" | "phase" | "agentId">,
) {
  return `${event.planningRunId}:${event.phase}:${event.agentId}`;
}

function removePendingPlanningProgress(
  progress: Record<string, PlanningAgentStatusEvent>,
  taskId: string,
) {
  return Object.fromEntries(
    Object.entries(progress).filter(
      ([, event]) => !(event.taskId === taskId && event.planningRunId === "pending"),
    ),
  );
}

export function reducePlanning(state: AppState, action: AppAction): AppState {
  switch (action.type) {
    case "planning/progressQueued": {
      const existing = Object.fromEntries(
        Object.entries(state.planningProgress).filter(([, event]) => event.taskId !== action.taskId),
      );
      const existingLogs = Object.fromEntries(
        Object.entries(state.planningLogs).filter(([, logs]) => logs[0]?.taskId !== action.taskId),
      );
      const queued = Object.fromEntries(
        action.agents.map((agent) => [
          `pending:planning:${agent.id}`,
          {
            taskId: action.taskId,
            planningRunId: "pending",
            agentId: agent.id,
            agentName: agent.name,
            phase: "planning" as const,
            status: "pending" as const,
            attempt: 1,
            startedAtMs: Date.now(),
          },
        ]),
      );
      return { ...state, planningLogs: existingLogs, planningProgress: { ...existing, ...queued } };
    }
    case "planning/progressUpdated": {
      const progress =
        action.event.status === "running"
          ? removePendingPlanningProgress(state.planningProgress, action.event.taskId)
          : state.planningProgress;
      return {
        ...state,
        planningProgress: { ...progress, [planningProgressKey(action.event)]: action.event },
      };
    }
    case "planning/logReceived": {
      const key = planningLogKey(action.event);
      const runLogs = state.planningLogs[key] ?? [];
      return {
        ...state,
        planningLogs: { ...state.planningLogs, [key]: [...runLogs, action.event].slice(-300) },
      };
    }
    case "planning/progressCleared":
      return {
        ...state,
        planningLogs: Object.fromEntries(
          Object.entries(state.planningLogs).filter(([, logs]) => logs[0]?.taskId !== action.taskId),
        ),
        planningProgress: Object.fromEntries(
          Object.entries(state.planningProgress).filter(([, event]) => event.taskId !== action.taskId),
        ),
      };
    default:
      return state;
  }
}
