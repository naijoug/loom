import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { DEFAULT_APP_SETTINGS, type AppSettings, type ThemeMode } from "../domain";
import { loadAppSettings, saveAppSettings } from "../hooks/useSettingsBridge";

type Theme = "light" | "dark";

interface ThemeContextType {
  theme: Theme;
  themeMode: ThemeMode;
  setThemeMode: (mode: ThemeMode) => void;
  toggleTheme: () => void;
}

interface ThemeProviderProps {
  children: ReactNode;
  forcedTheme?: Theme;
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

export function ThemeProvider({ children, forcedTheme }: ThemeProviderProps) {
  const [settings, setSettings] = useState<AppSettings>({ ...DEFAULT_APP_SETTINGS });
  const [system, setSystem] = useState<Theme>(systemTheme);
  const [themeMode, setThemeModeState] = useState<ThemeMode>(() => {
    if (forcedTheme) {
      return forcedTheme;
    }

    return resolveThemeMode(undefined, localStorage.getItem("loom-theme"));
  });
  const theme = forcedTheme ?? effectiveThemeForMode(themeMode, system);

  useEffect(() => {
    if (forcedTheme) {
      setThemeModeState(forcedTheme);
    }
  }, [forcedTheme]);

  useEffect(() => {
    if (forcedTheme) {
      return;
    }

    let cancelled = false;
    void loadAppSettings().then((loaded) => {
      if (cancelled) {
        return;
      }
      setSettings(loaded);
      setThemeModeState(resolveThemeMode(loaded.themeMode, localStorage.getItem("loom-theme")));
    });

    return () => {
      cancelled = true;
    };
  }, [forcedTheme]);

  // Apply theme to DOM
  useEffect(() => {
    document.documentElement.setAttribute("data-theme", theme);
  }, [theme]);

  // Listen for system theme changes if user hasn't explicitly set a preference
  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    
    const handleChange = (e: MediaQueryListEvent) => {
      if (forcedTheme) {
        return;
      }

      setSystem(e.matches ? "dark" : "light");
      if (themeMode === "system") {
        setThemeModeState("system");
      }
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [forcedTheme, themeMode]);

  const setThemeMode = (mode: ThemeMode) => {
    if (forcedTheme) {
      return;
    }

    const nextSettings = { ...settings, themeMode: mode };
    setSettings(nextSettings);
    setThemeModeState(mode);
    void saveAppSettings(nextSettings).then((saved) => {
      setSettings(saved);
      setThemeModeState(saved.themeMode);
    });
  };

  const toggleTheme = () => {
    if (forcedTheme) {
      return;
    }

    setThemeMode(theme === "light" ? "dark" : "light");
  };

  return (
    <ThemeContext.Provider value={{ theme, themeMode, setThemeMode, toggleTheme }}>
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
