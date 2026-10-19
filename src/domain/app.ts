export interface HealthCheckResult {
  status: "ok";
  app: string;
  version: string;
  backend: "tauri";
  timestampMs: number;
}

export type ThemeMode = "light" | "dark" | "system";

export interface AppSettings {
  themeMode: ThemeMode;
  confirmBeforeCommands: boolean;
  commandTimeoutSeconds: number;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  themeMode: "system",
  confirmBeforeCommands: true,
  commandTimeoutSeconds: 600,
};

export interface WorkflowStage {
  id: "planning" | "implementation" | "debugging" | "summary";
  title: string;
  status: "ready" | "pending" | "blocked";
  outcome: string;
}
