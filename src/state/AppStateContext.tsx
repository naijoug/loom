import { createContext, useContext, useMemo, useReducer, type Dispatch, type ReactNode } from "react";
import { appReducer, initialAppState, type AppAction, type AppState } from "./reducer";

interface AppStateContextValue {
  state: AppState;
  dispatch: Dispatch<AppAction>;
}

const AppStateContext = createContext<AppStateContextValue | null>(null);

interface AppStateProviderProps {
  children: ReactNode;
  initialStateOverride?: AppState;
}

export function AppStateProvider({ children, initialStateOverride }: AppStateProviderProps) {
  const [state, dispatch] = useReducer(appReducer, initialStateOverride ?? initialAppState);
  const value = useMemo(() => ({ state, dispatch }), [state]);

  return <AppStateContext.Provider value={value}>{children}</AppStateContext.Provider>;
}

export function useAppState() {
  const context = useContext(AppStateContext);

  if (!context) {
    throw new Error("useAppState must be used inside AppStateProvider");
  }

  return context;
}
