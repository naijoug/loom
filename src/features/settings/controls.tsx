import type { ReactNode } from "react";

export function SettingCard({ title, children }: { title: string; children: ReactNode }) {
  return <section className="setcard"><div className="setcard-title">{title}</div>{children}</section>;
}

export function Field({
  title,
  description,
  control,
}: {
  title: string;
  description?: string;
  control: ReactNode;
}) {
  return (
    <div className="settings-field-row">
      <div>
        <div className="settings-field-title">{title}</div>
        {description && <div className="settings-field-desc">{description}</div>}
      </div>
      <div className="settings-field-control">{control}</div>
    </div>
  );
}

export function Switch({ on }: { on: boolean }) {
  return <span className={`settings-switch ${on ? "on" : ""}`} />;
}

export function ToggleControl({
  checked,
  disabled,
  onChange,
}: {
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      className="settings-toggle-control"
      aria-pressed={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
    >
      <Switch on={checked} />
    </button>
  );
}

export function ThemePreview({
  label,
  active,
  mode,
  onClick,
}: {
  label: string;
  active: boolean;
  mode: "light" | "dark" | "system";
  onClick: () => void;
}) {
  return (
    <button type="button" className={`theme-card ${active ? "on" : ""}`} onClick={onClick}>
      <div className={`theme-preview theme-preview-${mode}`}>
        <div className="theme-preview-side" />
        <div className="theme-preview-main"><span /><span /><span /></div>
      </div>
      <div className="theme-card-caption">{label}<span /></div>
    </button>
  );
}
