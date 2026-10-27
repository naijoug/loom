import { Suspense, useEffect, useState } from "react";
import { useT } from "../i18n";
import { hasTauriRuntime } from "../hooks/runtime";
import { useAppState } from "../state/AppStateContext";
import type { AppView } from "../state/model";
import { ActivityRail, isMac } from "./ActivityRail";
import { FEATURES, SettingsPage } from "./features";
import "../styles/workspace.css";
import "./shell.css";

function ViewFallback() {
  const t = useT();
  return <div className="app-view-loading" role="status">{t("loading")}</div>;
}

/** Cmd+1…4 picks a feature in rail order, Cmd+, opens Settings (Ctrl on other platforms). */
export function shortcutView(event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "isComposing">, mac = isMac()): AppView | null {
  if (event.isComposing || event.altKey || event.shiftKey || !(mac ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey)) return null;
  if (event.key === ",") return "settings";
  const index = /^[1-4]$/.test(event.key) ? Number(event.key) - 1 : -1;
  return FEATURES[index]?.id ?? null;
}

/** App frame: the activity rail plus one pane per feature. Features mount on first visit (Chat at startup) and stay mounted; Settings mounts while open. */
export function AppShell() {
  const { state, dispatch } = useAppState();
  const current = state.app.currentView;
  const [visited, setVisited] = useState<ReadonlySet<AppView>>(() => new Set([current]));
  const select = (view: AppView) => dispatch({ type: "app/viewSelected", view });
  useEffect(() => { setVisited(previous => previous.has(current) ? previous : new Set(previous).add(current)); }, [current]);
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const view = shortcutView(event);
      if (!view) return;
      event.preventDefault();
      dispatch({ type: "app/viewSelected", view });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dispatch]);
  return <div className="app-shell" data-traffic-lights={hasTauriRuntime() && isMac() ? "" : undefined}>
    <ActivityRail current={current} onSelect={select} />
    <div className="app-shell-content">
      {FEATURES.map(feature => (feature.keepMounted || visited.has(feature.id) || current === feature.id) &&
        <div key={feature.id} className="app-shell-pane" data-feature={feature.id} hidden={current !== feature.id}>
          <Suspense fallback={<ViewFallback />}><feature.load /></Suspense>
        </div>)}
      {current === "settings" && <div className="app-shell-pane" data-feature="settings"><Suspense fallback={<ViewFallback />}><SettingsPage /></Suspense></div>}
    </div>
  </div>;
}
