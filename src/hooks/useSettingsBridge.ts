import { useCallback } from "react";
import { invokeCommand, TAURI_COMMANDS } from "../api";
import { DEFAULT_APP_SETTINGS, type AppSettings, type ThemeMode } from "../domain";
import { hasTauriRuntime } from "./runtime";

const FALLBACK_SETTINGS_KEY = "loom-app-settings";
const LEGACY_THEME_KEY = "loom-theme";

function isThemeMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

function normalizeSettings(value: Partial<AppSettings> | null | undefined): AppSettings {
  const timeout = Number(value?.commandTimeoutSeconds);
  return {
    themeMode: isThemeMode(value?.themeMode) ? value.themeMode : DEFAULT_APP_SETTINGS.themeMode,
    confirmBeforeCommands:
      typeof value?.confirmBeforeCommands === "boolean"
        ? value.confirmBeforeCommands
        : DEFAULT_APP_SETTINGS.confirmBeforeCommands,
    commandTimeoutSeconds:
      Number.isFinite(timeout) && timeout >= 5
        ? Math.min(3600, Math.round(timeout))
        : DEFAULT_APP_SETTINGS.commandTimeoutSeconds,
  };
}

function loadFallbackSettings(): AppSettings {
  try {
    const raw = localStorage.getItem(FALLBACK_SETTINGS_KEY);
    const parsed = raw ? JSON.parse(raw) as Partial<AppSettings> : null;
    const legacyTheme = localStorage.getItem(LEGACY_THEME_KEY);
    return normalizeSettings({
      ...parsed,
      themeMode: parsed?.themeMode ?? (isThemeMode(legacyTheme) ? legacyTheme : undefined),
    });
  } catch {
    return { ...DEFAULT_APP_SETTINGS };
  }
}

function saveFallbackSettings(settings: AppSettings) {
  localStorage.setItem(FALLBACK_SETTINGS_KEY, JSON.stringify(settings));
  if (settings.themeMode === "system") {
    localStorage.removeItem(LEGACY_THEME_KEY);
  } else {
    localStorage.setItem(LEGACY_THEME_KEY, settings.themeMode);
  }
}

export async function loadAppSettings(): Promise<AppSettings> {
  if (!hasTauriRuntime()) {
    return loadFallbackSettings();
  }

  return normalizeSettings(await invokeCommand<AppSettings>(TAURI_COMMANDS.loadAppSettings));
}

export async function saveAppSettings(settings: AppSettings): Promise<AppSettings> {
  const normalized = normalizeSettings(settings);
  saveFallbackSettings(normalized);

  if (!hasTauriRuntime()) {
    return normalized;
  }

  return normalizeSettings(
    await invokeCommand<AppSettings>(TAURI_COMMANDS.saveAppSettings, { settings: normalized }),
  );
}

export function useSettingsBridge() {
  const loadSettings = useCallback(() => loadAppSettings(), []);
  const saveSettings = useCallback((settings: AppSettings) => saveAppSettings(settings), []);

  return { loadSettings, saveSettings };
}
