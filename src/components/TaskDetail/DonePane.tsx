import { useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import {
  ArrowRight,
  CheckCircle2,
  Download,
  ExternalLink,
  FileText,
  FolderOpen,
  GitBranch,
  Plus,
  RefreshCw,
  ShieldCheck,
} from "lucide-react";
import type { ProjectSummary, Task, TaskSummaryValidation } from "../../domain";
import { formatRunStatus, WORKFLOW_COPY } from "../../copy/workflow";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { NewTaskModal } from "../Board";
import { Button } from "../common/Button";
import { TaskTimeline } from "./TaskTimeline";
import "./TaskDetail.css";

interface DonePaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

function shortStatus(run?: TaskSummaryValidation) {
  if (!run) {
    return "无验证";
  }
  return formatRunStatus(run.status, run.exitCode);
}

function repairLoops(task: Task) {
  return new Set(task.commandRuns.map((run) => run.loopId).filter(Boolean)).size;
}

function fileDelta(value?: number) {
  return typeof value === "number" ? value : "?";
}

export function DonePane({ project, task, readOnly = false }: DonePaneProps) {
  const { regenerateTaskSummary, exportTaskSummary, exportDiagnosticBundle } = useTaskBridge();
  const [followUpOpen, setFollowUpOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [includeDiagnosticLogs, setIncludeDiagnosticLogs] = useState(false);
  const summary = task.summary;
  const latestValidation = useMemo(
    () => summary?.validationEvidence[0],
    [summary?.validationEvidence],
  );
  const passedRuns = summary?.validationEvidence.filter((run) => run.status === "succeeded").length ?? 0;
  const failedRuns = summary?.validationEvidence.filter((run) => run.status === "failed").length ?? 0;

  async function regenerate() {
    setBusy(true);
    setNotice(null);
    const updated = await regenerateTaskSummary(project.path, task.id);
    setNotice(updated ? "交付总结已重新生成。" : "交付总结生成失败，请查看错误提示。");
    setBusy(false);
  }

  async function exportSummary(format: "markdown" | "json") {
    if (!summary) {
      return;
    }
    const extension = format === "markdown" ? "md" : "json";
    const targetPath = await save({
      defaultPath: `${task.title.replace(/[^\p{L}\p{N}._-]+/gu, "-") || "loom-task"}-summary.${extension}`,
      filters: [{ name: format === "markdown" ? "Markdown" : "JSON", extensions: [extension] }],
    });
    if (!targetPath) {
      return;
    }
    setBusy(true);
    const exported = await exportTaskSummary(project.path, task.id, targetPath, format);
    setNotice(exported ? `已导出到 ${exported}` : "导出失败，请查看错误提示。");
    setBusy(false);
  }

  async function exportDiagnostics() {
    const targetPath = await save({
      defaultPath: `${task.title.replace(/[^\p{L}\p{N}._-]+/gu, "-") || "loom-task"}-diagnostics.json`,
      filters: [{ name: "JSON", extensions: ["json"] }],
    });
    if (!targetPath) return;
    setBusy(true);
    setNotice(null);
    const exported = await exportDiagnosticBundle(
      project.path,
      task.id,
      targetPath,
      includeDiagnosticLogs,
    );
    setNotice(exported ? `脱敏诊断包已导出到 ${exported}` : "诊断包导出失败，请查看错误提示。");
    setBusy(false);
  }

  return (
    <div className="done-pane delivery">
      <div className="done-header dl-head">
        <div className="dl-head-left">
          <span className="loom-agent-avatar claude" aria-hidden="true">Cl</span>
          <div className="dl-head-info">
            <div className="done-kicker">交付总结</div>
            <h1>{task.title}</h1>
            <p>
              {project.name} · 已交付
              {summary ? ` · ${new Date(summary.generatedAtMs).toLocaleString()}` : " · 等待生成总结"}
            </p>
          </div>
        </div>
        <div className="dl-head-right">
          <span className="dl-status-pill"><span className="dl-status-dot" />已验收</span>
          {summary && (
            <>
              <Button variant="ghost" iconLeft={<ExternalLink size={14} />} onClick={() => void openPath(summary.markdownPath)}>
                打开总结
              </Button>
              <Button variant="ghost" iconLeft={<FolderOpen size={14} />} onClick={() => void revealItemInDir(summary.markdownPath)}>
                定位文件
              </Button>
              <Button variant="ghost" iconLeft={<Download size={14} />} disabled={busy} onClick={() => void exportSummary("markdown")}>
                导出 MD
              </Button>
              <Button variant="ghost" iconLeft={<Download size={14} />} disabled={busy} onClick={() => void exportSummary("json")}>
                导出 JSON
              </Button>
            </>
          )}
          {!readOnly && (
            <Button variant="ghost" iconLeft={<RefreshCw size={14} />} disabled={busy} onClick={() => void regenerate()}>
              重新生成
            </Button>
          )}
          {!readOnly && (
            <Button variant="primary" iconLeft={<Plus size={14} />} onClick={() => setFollowUpOpen(true)}>
              {WORKFLOW_COPY.actions.createFollowUp}
            </Button>
          )}
        </div>
      </div>

      {notice && <div className="testing-inline-note">{notice}</div>}
      {!summary ? (
        <section className="dl-block done-summary-missing">
          <h2>尚未生成持久化总结</h2>
          <p>点击“重新生成”以从当前任务记录和 Git baseline 创建 JSON 与 Markdown 交付物。</p>
        </section>
      ) : (
        <>
          <div className="dl-metrics">
            <div className="metric-card">
              <span className="metric-label">变更文件</span>
              <span className="metric-value">{summary.changedFiles.length}</span>
              <span className="metric-sub">真实 Git / 证据归因</span>
            </div>
            <div className="metric-card">
              <span className="metric-label">代码增删</span>
              <span className="metric-value">+{summary.totalAdditions} / -{summary.totalDeletions}</span>
              <span className="metric-sub">numstat</span>
            </div>
            <div className="metric-card">
              <span className="metric-label">验证</span>
              <span className="metric-value">{passedRuns}<span className="metric-unit"> 次通过</span></span>
              <span className="metric-sub">{failedRuns} failed</span>
            </div>
            <div className="metric-card">
              <span className="metric-label">修复循环</span>
              <span className="metric-value">{repairLoops(task)}</span>
              <span className="metric-sub">{shortStatus(latestValidation)}</span>
            </div>
          </div>

          <div className="dl-grid">
            <div className="dl-col">
              <section className="dl-block">
                <header className="dl-block-head">
                  <h2>完成的需求</h2>
                  <span className="dl-block-sub">{summary.completedTodos.length}/{task.planTodos.length}</span>
                </header>
                <ul className="req-list">
                  {summary.completedTodos.map((todo, index) => (
                    <li className="req-item is-done" key={`${todo}-${index}`}>
                      <span className="req-check"><CheckCircle2 size={12} /></span>
                      <span className="req-text">{todo}</span>
                    </li>
                  ))}
                  {summary.completedTodos.length === 0 && <li className="req-item">暂无已完成子任务记录。</li>}
                </ul>
              </section>

              <section className="dl-block">
                <header className="dl-block-head">
                  <h2>变更文件</h2>
                  <span className="dl-block-sub">{summary.changedFiles.length}</span>
                </header>
                <ul className="file-list">
                  {summary.changedFiles.map((file) => (
                    <li className="file-row" key={file.path}>
                      <FileText size={13} className="file-ico" />
                      <span className="file-path loom-mono" title={file.path}>{file.path}</span>
                      <span className="file-stat file-add">+{fileDelta(file.additions)}</span>
                      <span className="file-stat file-del">-{fileDelta(file.deletions)}</span>
                      <span className="file-stat">{file.attribution}</span>
                    </li>
                  ))}
                  {summary.changedFiles.length === 0 && <li className="file-row empty">没有检测到变更文件。</li>}
                </ul>
              </section>

              <section className="dl-block">
                <header className="dl-block-head"><h2>验证证据</h2><span className="dl-block-sub">{shortStatus(latestValidation)}</span></header>
                <div className="proof-box loom-mono">
                  {summary.validationEvidence.map((validation) => (
                    <div className="delivery-validation" key={validation.runId}>
                      <div className="proof-line"><span className="proof-dim">$</span> {validation.command}</div>
                      <div className="proof-line">
                        <span className={validation.status === "succeeded" ? "proof-green" : "proof-red"}>{shortStatus(validation)}</span>
                      </div>
                      {validation.stdoutLogRef && <div className="proof-line proof-dim">{validation.stdoutLogRef}</div>}
                      {validation.stderrLogRef && <div className="proof-line proof-dim">{validation.stderrLogRef}</div>}
                    </div>
                  ))}
                  {summary.validationEvidence.length === 0 && <div className="proof-line proof-dim">没有记录验证命令。</div>}
                </div>
              </section>
            </div>

            <div className="dl-col">
              <section className="dl-block">
                <header className="dl-block-head"><h2>关键决策</h2><GitBranch size={14} /></header>
                <ul className="decision-list">
                  {summary.decisions.map((decision, index) => (
                    <li className="decision-item" key={`${decision.kind}-${decision.title}-${index}`}>
                      <span className="decision-tag">{decision.title}</span>
                      <span className="decision-text">{decision.detail}</span>
                    </li>
                  ))}
                  {summary.decisions.length === 0 && <li>暂无关键决策记录。</li>}
                </ul>
              </section>

              <section className="dl-block">
                <header className="dl-block-head"><h2>实施 Review</h2><ShieldCheck size={14} /></header>
                <ul className="decision-list">
                  {summary.reviews.map((review, index) => (
                    <li className="decision-item" key={`${review.reviewer}-${index}`}>
                      <span className="decision-tag">{review.reviewer} · {review.status}</span>
                      <span className="decision-text">{review.summary} · {review.findingCount} findings</span>
                    </li>
                  ))}
                  {summary.reviews.length === 0 && <li>暂无实施 Review 记录。</li>}
                </ul>
              </section>

              <section className="dl-block">
                <header className="dl-block-head"><h2>剩余风险与建议</h2><ShieldCheck size={14} /></header>
                <ul className="risk-list">
                  {summary.remainingRisks.map((risk, index) => (
                    <li className="risk-item risk-medium" key={`risk-${index}`}><span /><p>{risk}</p></li>
                  ))}
                  {summary.recommendations.map((item, index) => (
                    <li className="risk-item risk-low" key={`recommendation-${index}`}><span /><p>{item}</p></li>
                  ))}
                </ul>
              </section>

              <section className="dl-block">
                <header className="dl-block-head"><h2>问题诊断</h2><ShieldCheck size={14} /></header>
                <p>导出版本、系统、任务状态、运行元数据和策略判定；项目路径与敏感字段会自动脱敏。</p>
                <label className="settings-check">
                  <input
                    type="checkbox"
                    checked={includeDiagnosticLogs}
                    onChange={(event) => setIncludeDiagnosticLogs(event.target.checked)}
                  />
                  <span>包含最近日志尾部（最多 20 份，每份 200 行）</span>
                </label>
                <Button
                  type="button"
                  variant="ghost"
                  iconLeft={<Download size={14} />}
                  disabled={busy}
                  onClick={() => void exportDiagnostics()}
                >
                  导出脱敏诊断包
                </Button>
              </section>

              <section className="dl-block">
                <header className="dl-block-head"><h2>任务时间线</h2><ArrowRight size={14} /></header>
                <TaskTimeline task={task} maxItems={7} />
              </section>
            </div>
          </div>
        </>
      )}

      {followUpOpen && <NewTaskModal project={project} onClose={() => setFollowUpOpen(false)} />}
    </div>
  );
}
