import { useT } from "../../i18n";
import "./ErrorText.css";

/**
 * Splits a backend error into a readable summary and the raw output behind it.
 * Backend errors use "summary, blank line, details"; older Agent runs stored
 * `agent exited with status <status>; stderr: <raw stderr>`.
 */
export function splitError(text: string): { summary: string; details?: string } {
  const value = text.replace(/^Error:\s*/, "").trim();
  const blank = value.indexOf("\n\n");
  if (blank >= 0) return { summary: value.slice(0, blank).trim(), details: value.slice(blank + 2).trim() || undefined };
  const stderr = value.indexOf("; stderr: ");
  if (stderr >= 0) return { summary: value.slice(0, stderr).replace("status exit status:", "status").trim(), details: value.slice(stderr + 10).trim() || undefined };
  const line = value.indexOf("\n");
  if (line >= 0) return { summary: value.slice(0, line).trim(), details: value.slice(line + 1).trim() || undefined };
  return { summary: value };
}

/** Shows an error's summary, with its raw details behind a collapsed toggle. */
export function ErrorText({ text, code, alert, className }: { text: string; code?: string | null; alert?: boolean; className?: string }) {
  const t = useT();
  const { summary, details } = splitError(text);
  return <div className={className ? `error-text ${className}` : "error-text"} role={alert ? "alert" : undefined}>
    <span>{summary}{code ? ` (${code})` : ""}</span>
    {details && <details className="error-text-details"><summary>{t("errorDetails")}</summary><pre>{details}</pre></details>}
  </div>;
}
