import { Settings, type LucideIcon } from "lucide-react";
import { useT } from "../i18n";
import type { AppView } from "../state/model";
import { FEATURES } from "./features";

export const isMac = () => typeof navigator !== "undefined" && /Mac/.test(navigator.userAgent);
/** "⌘1" on macOS, "Ctrl+1" elsewhere. */
export const shortcutLabel = (key: string) => (isMac() ? `⌘${key}` : `Ctrl+${key}`);

function RailButton({
  view,
  icon: Icon,
  label,
  shortcut,
  active,
  onClick,
}: {
  view: AppView;
  icon: LucideIcon;
  label: string;
  shortcut: string;
  active: boolean;
  onClick: () => void;
}) {
  const t = useT();
  const hint = t("railItemHint", { label, shortcut });
  return (
    <button
      type="button"
      className="activity-rail-item"
      data-view={view}
      aria-label={hint}
      title={hint}
      aria-current={active ? "page" : undefined}
      onClick={onClick}
    >
      <Icon size={18} strokeWidth={1.75} aria-hidden="true" />
    </button>
  );
}

/** Codex-style left rail: features on top in registry order, Settings at the bottom. */
export function ActivityRail({ current, onSelect }: { current: AppView; onSelect: (view: AppView) => void }) {
  const t = useT();
  return (
    <nav className="activity-rail" aria-label={t("railLabel")} data-testid="activity-rail">
      <div className="activity-rail-drag" data-tauri-drag-region />
      {FEATURES.map((feature, index) => (
        <RailButton
          key={feature.id}
          view={feature.id}
          icon={feature.icon}
          label={t(feature.labelKey)}
          shortcut={shortcutLabel(String(index + 1))}
          active={current === feature.id}
          onClick={() => onSelect(feature.id)}
        />
      ))}
      <div className="activity-rail-spacer" data-tauri-drag-region />
      <RailButton
        view="settings"
        icon={Settings}
        label={t("settings")}
        shortcut={shortcutLabel(",")}
        active={current === "settings"}
        onClick={() => onSelect("settings")}
      />
    </nav>
  );
}
