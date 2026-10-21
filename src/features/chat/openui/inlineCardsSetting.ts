import { useSyncExternalStore } from "react";
import { loadAppSettings, patchAppSettings } from "../../../hooks/useSettingsBridge";

// Shared by Chat and Settings, which stay mounted together; kept free of OpenUI imports.
let enabled = false;
let loading: Promise<void> | null = null;
const listeners = new Set<() => void>();

function publish(next: boolean) {
  enabled = next;
  listeners.forEach(listener => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  loading ??= loadAppSettings().then(settings => publish(settings.experimentalInlineCards)).catch(() => {});
  return () => { listeners.delete(listener); };
}

export function useInlineCardsEnabled() {
  return useSyncExternalStore(subscribe, () => enabled, () => enabled);
}

export async function setInlineCardsEnabled(next: boolean) {
  const previous = enabled;
  publish(next);
  try { publish((await patchAppSettings({ experimentalInlineCards: next })).experimentalInlineCards); }
  catch (error) { publish(previous); throw error; }
}
