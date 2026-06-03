import { createContext, useContext, useEffect, useState, type ReactNode } from "react";

type Theme = "light" | "dark";

interface ThemeContextType {
  theme: Theme;
  toggleTheme: () => void;
}

interface ThemeProviderProps {
  children: ReactNode;
  forcedTheme?: Theme;
}

const ThemeContext = createContext<ThemeContextType | undefined>(undefined);

export function ThemeProvider({ children, forcedTheme }: ThemeProviderProps) {
  const [theme, setTheme] = useState<Theme>(() => {
    if (forcedTheme) {
      return forcedTheme;
    }

    const saved = localStorage.getItem("loom-theme");
    if (saved) return saved as Theme;
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  });

  useEffect(() => {
    if (forcedTheme) {
      setTheme(forcedTheme);
    }
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

      const saved = localStorage.getItem("loom-theme");
      if (!saved) {
        setTheme(e.matches ? "dark" : "light");
      }
    };

    mediaQuery.addEventListener("change", handleChange);
    return () => mediaQuery.removeEventListener("change", handleChange);
  }, [forcedTheme]);

  const toggleTheme = () => {
    if (forcedTheme) {
      return;
    }

    setTheme((prev) => {
      const nextTheme = prev === "light" ? "dark" : "light";
      localStorage.setItem("loom-theme", nextTheme);
      return nextTheme;
    });
  };

  return (
    <ThemeContext.Provider value={{ theme, toggleTheme }}>
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
