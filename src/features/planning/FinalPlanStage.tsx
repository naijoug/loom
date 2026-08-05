import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { ExternalLink, FileText } from "lucide-react";
import type { Task } from "../../domain";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { PLAN_PREVIEW_SANDBOX, wireIframeHashNavigation } from "../../utils/iframeNavigation";
import { Button } from "../../components/common/Button";
import { WORKFLOW_COPY } from "../../copy/workflow";

function escapePreviewHtml(value: string) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function renderInlineMarkdown(value: string) {
  return escapePreviewHtml(value).replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");
}

function renderFallbackPlanHtml(markdown: string) {
  const blocks: string[] = [];
  let listType: "ul" | "ol" | null = null;
  const closeList = () => {
    if (listType) {
      blocks.push(`</${listType}>`);
      listType = null;
    }
  };

  for (const rawLine of markdown.split(/\r?\n/)) {
    const line = rawLine.trim();
    const heading = line.match(/^(#{1,4})\s+(.+)$/);
    const bullet = line.match(/^[-*]\s+(.+)$/);
    const ordered = line.match(/^\d+[.)]\s+(.+)$/);
    if (heading) {
      closeList();
      const level = Math.min(heading[1].length + 1, 4);
      blocks.push(`<h${level}>${renderInlineMarkdown(heading[2])}</h${level}>`);
    } else if (bullet || ordered) {
      const nextType = bullet ? "ul" : "ol";
      if (listType !== nextType) {
        closeList();
        listType = nextType;
        blocks.push(`<${nextType}>`);
      }
      blocks.push(`<li>${renderInlineMarkdown((bullet ?? ordered)![1])}</li>`);
    } else if (line) {
      closeList();
      blocks.push(`<p>${renderInlineMarkdown(line)}</p>`);
    } else {
      closeList();
    }
  }
  closeList();
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><style>
    :root{color-scheme:light dark;--bg:#f7f8fb;--surface:#fff;--text:#17202a;--muted:#667085;--border:#d9e0ea;--accent:#2563eb}
    @media(prefers-color-scheme:dark){:root{--bg:#15171c;--surface:#1c1f26;--text:#eef2f7;--muted:#aab3c2;--border:#303642;--accent:#8ab4ff}}
    *{box-sizing:border-box}body{margin:0;background:var(--bg);color:var(--text);font:14px/1.6 Inter,-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
    article{max-width:780px;margin:0 auto;padding:24px 28px 48px}h2{margin:0 0 22px;font-size:24px;line-height:1.24}h3{margin:24px 0 8px;padding-top:16px;border-top:1px solid var(--border);font-size:17px}h4{margin:18px 0 6px;font-size:15px}p{margin:7px 0;color:var(--muted)}ul,ol{margin:8px 0 14px;padding-left:22px}li{margin:5px 0}strong{color:var(--text)}
  </style></head><body><article>${blocks.join("")}</article></body></html>`;
}

function StageMarker({ icon, label }: { icon: ReactNode; label: string }) {
  return <div className="timeline-stage-marker"><span className="timeline-stage-icon">{icon}</span><span className="timeline-stage-label">{label}</span></div>;
}

interface FinalPlanStageProps {
  projectPath: string;
  task: Task;
  busy: boolean;
  canRerunReviews: boolean;
  readOnly: boolean;
}

export function FinalPlanStage({ projectPath, task, busy, canRerunReviews, readOnly }: FinalPlanStageProps) {
  const { dispatch } = useAppState();
  const { runPlanReviews } = useAgentBridge();
  const { confirmPlan, openPlanHtml, openPlanViewer, readPlanHtml, recordPlanningDecision } = useTaskBridge();
  const [html, setHtml] = useState<string | null>(null);
  const [decision, setDecision] = useState("");

  useEffect(() => {
    if (!task.finalPlanPath) {
      setHtml(null);
      return;
    }
    let cancelled = false;
    void readPlanHtml(projectPath, task.finalPlanPath).then((value) => {
      if (!cancelled) setHtml(value);
    });
    return () => { cancelled = true; };
  }, [projectPath, readPlanHtml, task.finalPlanPath, task.updatedAtMs]);

  if (!task.finalPlan) return null;
  const fallback = renderFallbackPlanHtml(task.finalPlan);

  async function handleRecordDecision(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!decision.trim()) return;
    const updated = await recordPlanningDecision({
      projectPath,
      taskId: task.id,
      title: "规划决策",
      content: decision.trim(),
    });
    if (updated) setDecision("");
  }

  async function handleCreateTasks() {
    const updated = await confirmPlan(projectPath, task.id);
    if (updated) dispatch({ type: "app/viewSelected", view: "board" });
  }

  return (
    <div className="timeline-stage timeline-final-stage">
      <StageMarker icon={<FileText size={13} />} label="最终计划" />
      <div className="timeline-stage-body">
        <div className="timeline-final-card">
          <div className="timeline-final-toolbar">
            <span className="timeline-final-path" title={task.finalPlanPath}>
              {task.finalPlanPath?.split("/").slice(-2).join("/")}
            </span>
            <span className="timeline-final-actions">
              <Button type="button" variant="ghost" disabled={!task.finalPlanPath} onClick={() => task.finalPlanPath && void openPlanViewer(projectPath, task.finalPlanPath)}>
                在 Loom 中打开
              </Button>
              <Button type="button" variant="ghost" iconRight={<ExternalLink size={13} />} disabled={!task.finalPlanHtmlPath} onClick={() => task.finalPlanHtmlPath && void openPlanHtml(projectPath, task.finalPlanHtmlPath)}>
                在浏览器中打开
              </Button>
              {canRerunReviews && !readOnly && (
                <Button type="button" variant="ghost" disabled={busy} onClick={() => void runPlanReviews(projectPath, task.id)}>
                  重新评审
                </Button>
              )}
            </span>
          </div>
          <details className="timeline-final-preview" open>
            <summary>计划预览</summary>
            <iframe title="最终计划预览" sandbox={PLAN_PREVIEW_SANDBOX} srcDoc={html ?? fallback} onLoad={(event) => wireIframeHashNavigation(event.currentTarget)} />
          </details>

          {task.planningDecisions.length > 0 && (
            <ul className="timeline-decision-list">
              {task.planningDecisions.map((item) => <li key={item.id}><strong>{item.title}</strong><span>{item.content}</span></li>)}
            </ul>
          )}

          {!readOnly && (
            <details className="timeline-decision-form">
              <summary>补充人工决策</summary>
              <form onSubmit={handleRecordDecision}>
                <textarea value={decision} onChange={(event) => setDecision(event.target.value)} placeholder="记录范围、取舍、风险或必须遵守的约束。" />
                <Button type="submit" variant="ghost" disabled={!decision.trim()}>记录决策</Button>
              </form>
            </details>
          )}

          {!readOnly && (
            <div className="timeline-final-cta">
              <span>确认后会从最终计划生成可执行任务，并进入实施阶段。</span>
              <Button type="button" variant="primary" disabled={busy} onClick={() => void handleCreateTasks()}>
                {WORKFLOW_COPY.actions.confirmPlan}
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
