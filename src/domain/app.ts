export interface HealthCheckResult {
  status: "ok";
  app: string;
  version: string;
  backend: "tauri";
  timestampMs: number;
}

export interface WorkflowStage {
  id: "planning" | "implementation" | "debugging" | "summary";
  title: string;
  status: "ready" | "pending" | "blocked";
  outcome: string;
}
