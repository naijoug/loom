export interface HealthCheckResult {
  status: "ok";
  app: string;
  version: string;
  backend: "tauri";
  timestampMs: number;
}

export type ThemeMode = "light" | "dark" | "system";

export interface AppSettings {
  locale: "en" | "zh-CN";
  themeMode: ThemeMode;
  confirmBeforeCommands: boolean;
  commandTimeoutSeconds: number;
  /** Opt-in OpenUI inline cards; only adapters with `inlineCards` receive the prompt. */
  experimentalInlineCards: boolean;
}

export const DEFAULT_APP_SETTINGS: AppSettings = {
  locale: "en",
  themeMode: "system",
  confirmBeforeCommands: true,
  commandTimeoutSeconds: 600,
  experimentalInlineCards: false,
};

export interface WorkflowStage {
  id: "planning" | "implementation" | "debugging" | "summary";
  title: string;
  status: "ready" | "pending" | "blocked";
  outcome: string;
}
