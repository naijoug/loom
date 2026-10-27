import { useT } from "../../i18n";

/** Splits a backend error into a readable summary and the raw output behind it. */
export function splitError(text: string): { summary: string; details?: string } {
  const value = text.replace(/^Error:\s*/, "").trim();
  const blank = value.indexOf("\n\n");
  if (blank >= 0) return { summary: value.slice(0, blank).trim(), details: value.slice(blank + 2).trim() || undefined };
  // Agent runs report `agent exited with status <status>; stderr: <raw stderr>`.
  const stderr = value.indexOf("; stderr: ");
  if (stderr >= 0) return { summary: value.slice(0, stderr).replace("status exit status:", "status").trim(), details: value.slice(stderr + 10).trim() || undefined };
  const line = value.indexOf("\n");
  if (line >= 0) return { summary: value.slice(0, line).trim(), details: value.slice(line + 1).trim() || undefined };
  return { summary: value };
}

export function ErrorText({ text, alert, className = "learn-error" }: { text: string; alert?: boolean; className?: string }) {
  const t = useT();
  const { summary, details } = splitError(text);
  return <div className={className} role={alert ? "alert" : undefined}>
    <span>{summary}</span>
    {details && <details className="learn-error-details"><summary>{t("learnErrorDetails")}</summary><pre>{details}</pre></details>}
  </div>;
}
