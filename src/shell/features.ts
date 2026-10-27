import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import { GraduationCap, MessageSquare, type LucideIcon } from "lucide-react";
import type { MessageKey } from "../i18n/en";

export interface FeatureEntry {
  id: string;
  icon: LucideIcon;
  labelKey: MessageKey;
  load: LazyExoticComponent<ComponentType>;
  /** Mounted from startup instead of on first visit. Every feature stays mounted once shown, so switching keeps its state. */
  keepMounted: boolean;
}

/**
 * Activity rail features, in rail order (Cmd+1… follows it). A feature joins only once delivered: no placeholders.
 * Features never import each other; shared UI lives in src/components.
 */
export const FEATURES = [
  // Chat stays mounted so running turns, drafts and selection survive switching (and Chat loads agents and diagnostics at startup).
  { id: "chat", icon: MessageSquare, labelKey: "chat", load: lazy(() => import("../features/chat").then(module => ({ default: module.ChatPage }))), keepMounted: true },
  { id: "learn", icon: GraduationCap, labelKey: "learn", load: lazy(() => import("../features/learn").then(module => ({ default: module.LearnPage }))), keepMounted: false },
] as const satisfies readonly FeatureEntry[];

export type FeatureId = (typeof FEATURES)[number]["id"];

export const SettingsPage = lazy(() => import("../features/settings/SettingsPage").then(module => ({ default: module.SettingsPage })));

export function isFeatureId(view: string): view is FeatureId {
  return FEATURES.some(feature => feature.id === view);
}
