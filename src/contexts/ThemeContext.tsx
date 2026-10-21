import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { type ThemeMode } from "../domain";
import { loadAppSettings, patchAppSettings } from "../hooks/useSettingsBridge";

type Theme = "light" | "dark";

interface ThemeContextType {
  theme: Theme;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  settingsError: string | null;
}

interface ThemeProviderProps {
  children: ReactNode;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

function systemTheme() {
  if (typeof window === "undefined" || !window.matchMedia) {
    return "light";
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export function isThemeMode(value: unknown): value is ThemeMode {
  return value === "light" || value === "dark" || value === "system";
}

export function resolveThemeMode(settingsThemeMode?: unknown, legacyTheme?: unknown): ThemeMode {
  if (isThemeMode(settingsThemeMode)) {
    return settingsThemeMode;
  }
  if (legacyTheme === "light" || legacyTheme === "dark") {
    return legacyTheme;
  }
  return "system";
}

export function effectiveThemeForMode(mode: ThemeMode, system: Theme): Theme {
  return mode === "system" ? system : mode;
}

export function ThemeProvider({ children }: ThemeProviderProps) {
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [system, setSystem] = useState<Theme>(systemTheme);
  const [themeMode, setThemeModeState] = useState<ThemeMode>(() => resolveThemeMode(undefined, localStorage.getItem("loom-theme")));
  const theme = effectiveThemeForMode(themeMode, system);

  useEffect(() => {
    let cancelled = false;
    void loadAppSettings().then((loaded) => {
      if (cancelled) {
        return;
      }
      setThemeModeState(resolveThemeMode(loaded.themeMode, localStorage.getItem("loom-theme")));
    }).catch(error => { if (!cancelled) setSettingsError(String(error)); });

    return () => {
      cancelled = true;
    };
  }, []);

  // Apply theme to DOM
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  // Listen for system theme changes if user hasn't explicitly set a preference
  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    
    const handleChange = (e: MediaQueryListEvent) => setSystem(e.matches ? "dark" : "light");

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, []);

  const setThemeMode = (mode: ThemeMode) => {
    setSettingsError(null);
    setThemeModeState(mode);
    void patchAppSettings({ themeMode: mode }).then((saved) => {
      setThemeModeState(saved.themeMode);
    }).catch(error => { setThemeModeState(themeMode); setSettingsError(String(error)); });
  };

  return (
    <ThemeContext.Provider value={{ theme, themeMode, setThemeMode, settingsError }}>
      {children}
    </ThemeContext.Provider>
  );
}

export function useTheme() {
  const context = useContext(ThemeContext);
  if (context === undefined) {
    throw new Error("useTheme must be used within a ThemeProvider");
  }
  return context;
}
