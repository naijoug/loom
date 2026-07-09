import { useMemo, useState } from "react";
import {
  ArrowRight,
  CheckCircle2,
  FileText,
  GitBranch,
  Plus,
  ShieldCheck,
} from "lucide-react";
import type { CommandRun, ProjectSummary, Task } from "../../domain";
import { NewTaskModal } from "../Board";
import { Button } from "../common/Button";
import { TaskTimeline } from "./TaskTimeline";
import "./TaskDetail.css";

interface DonePaneProps {
  project: ProjectSummary;
  task: Task;
  readOnly?: boolean;
}

function latestValidation(task: Task) {
  return (
    task.commandRuns
      .filter((run) => run.intent === "validation" || run.intent === undefined)
      .slice()
      .sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ??
    task.commandRuns.slice().sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ??
    null
  );
}

function shortStatus(run: CommandRun | null) {
  if (!run) {
    return "无验证";
  }
  return typeof run.exitCode === "number" ? `${run.status} · exit ${run.exitCode}` : run.status;
}

function evidenceFiles(task: Task) {
  const paths = [
    task.finalPlanPath,
    task.finalPlanHtmlPath,
    ...task.agentInvocations.flatMap((invocation) => [
      invocation.planPath,
      invocation.evidenceRef,
      invocation.stderrRef,
    ]),
    ...task.commandRuns.flatMap((run) => [run.stdoutLogRef, run.stderrLogRef]),
  ].filter((path): path is string => Boolean(path));

  return Array.from(new Set(paths)).slice(0, 6);
}

function repairLoops(task: Task) {
  return new Set(task.commandRuns.map((run) => run.loopId).filter(Boolean)).size;
}

function riskItems(task: Task) {
  const reviewRisks = task.planReviews
    .filter((review) => review.severity !== "info")
    .slice(-3)
    .map((review) => ({
      tone: review.severity === "blocker" ? "high" : "medium",
      text: review.finding,
    }));
  const failedRun = task.commandRuns
    .slice()
    .reverse()
    .find((run) => run.status === "failed" || run.status === "cancelled");

  if (failedRun) {
    reviewRisks.push({
      tone: "medium",
      text: `${failedRun.command} · ${shortStatus(failedRun)}`,
    });
  }

  return reviewRisks.length > 0
    ? reviewRisks
    : [{ tone: "low", text: "暂无剩余风险记录。" }];
}

export function DonePane({ project, task, readOnly = false }: DonePaneProps) {
  const [followUpOpen, setFollowUpOpen] = useState(false);
  const validation = useMemo(() => latestValidation(task), [task]);
  const completedTodos = task.planTodos.filter((todo) => todo.status === "done");
  const files = evidenceFiles(task);
  const passedRuns = task.commandRuns.filter((run) => run.status === "succeeded").length;
  const failedRuns = task.commandRuns.filter((run) => run.status === "failed").length;
  const risks = riskItems(task);

  return (
    <div className="done-pane delivery">
      <div className="done-header dl-head">
        <div className="dl-head-left">
          <span className="loom-agent-avatar claude" aria-hidden="true">Cl</span>
          <div className="dl-head-info">
            <div className="done-kicker">交付总结</div>
            <h1>{task.title}</h1>
            <p>{project.name} · 已交付</p>
          </div>
        </div>
        <div className="dl-head-right">
          <span className="dl-status-pill">
            <span className="dl-status-dot" />
            已验收
          </span>
          {!readOnly && (
            <Button
              variant="primary"
              iconLeft={<Plus size={14} />}
              onClick={() => setFollowUpOpen(true)}
            >
              开始后续任务
            </Button>
          )}
        </div>
      </div>

      <div className="dl-metrics">
        <div className="metric-card">
          <span className="metric-label">变更文件</span>
          <span className="metric-value">{files.length}</span>
          <span className="metric-sub">条证据</span>
        </div>
        <div className="metric-card">
          <span className="metric-label">代码增删</span>
          <span className="metric-value">待统计</span>
          <span className="metric-sub">由 diff 证据补齐</span>
        </div>
        <div className="metric-card">
          <span className="metric-label">测试</span>
          <span className="metric-value">{passedRuns}<span className="metric-unit"> passed</span></span>
          <span className="metric-sub">{failedRuns} failed</span>
        </div>
        <div className="metric-card">
          <span className="metric-label">修复循环</span>
          <span className="metric-value">{repairLoops(task)}</span>
          <span className="metric-sub">{shortStatus(validation)}</span>
        </div>
      </div>

      <div className="dl-grid">
        <div className="dl-col">
          <section className="dl-block">
            <header className="dl-block-head">
              <h2>完成的需求</h2>
              <span className="dl-block-sub">{completedTodos.length}/{task.planTodos.length}</span>
            </header>
            {completedTodos.length > 0 ? (
              <ul className="req-list">
                {completedTodos.map((todo) => (
                  <li className="req-item is-done" key={todo.id}>
                    <span className="req-check"><CheckCircle2 size={12} /></span>
                    <span className="req-text">{todo.title}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p>暂无已完成子任务记录。</p>
            )}
          </section>

          <section className="dl-block">
            <header className="dl-block-head">
              <h2>变更文件</h2>
              <span className="dl-block-sub">{files.length}</span>
            </header>
            <ul className="file-list">
              {files.map((path) => (
                <li className="file-row" key={path}>
                  <FileText size={13} className="file-ico" />
                  <span className="file-path loom-mono">{path}</span>
                  <span className="file-stat file-add">证据</span>
                </li>
              ))}
              {files.length === 0 && <li className="file-row empty">暂无文件证据。</li>}
            </ul>
          </section>

          <section className="dl-block">
            <header className="dl-block-head">
              <h2>验证证据</h2>
              <span className="dl-block-sub">{shortStatus(validation)}</span>
            </header>
            <div className="proof-box loom-mono">
              {validation ? (
                <>
                  <div className="proof-line"><span className="proof-dim">$</span> {validation.command}</div>
                  <div className="proof-line">
                    <span className={validation.status === "succeeded" ? "proof-green" : "proof-red"}>
                      {shortStatus(validation)}
                    </span>
                  </div>
                  {validation.stdoutLogRef && <div className="proof-line proof-dim">{validation.stdoutLogRef}</div>}
                  {validation.stderrLogRef && <div className="proof-line proof-dim">{validation.stderrLogRef}</div>}
                </>
              ) : (
                <div className="proof-line proof-dim">没有记录验证命令。</div>
              )}
            </div>
          </section>
        </div>

        <div className="dl-col">
          <section className="dl-block">
            <header className="dl-block-head">
              <h2>关键决策</h2>
              <GitBranch size={14} />
            </header>
            {task.planningDecisions.length > 0 ? (
              <ul className="decision-list">
                {task.planningDecisions.slice(-5).map((decision) => (
                  <li className="decision-item" key={decision.id}>
                    <span className="decision-tag">{decision.title}</span>
                    <span className="decision-text">{decision.content}</span>
                  </li>
                ))}
              </ul>
            ) : (
              <p>暂无关键决策记录。</p>
            )}
          </section>

          <section className="dl-block">
            <header className="dl-block-head">
              <h2>剩余风险</h2>
              <ShieldCheck size={14} />
            </header>
            <ul className="risk-list">
              {risks.map((risk, index) => (
                <li className={`risk-item risk-${risk.tone}`} key={`${risk.tone}-${index}`}>
                  <span />
                  <p>{risk.text}</p>
                </li>
              ))}
            </ul>
          </section>

          <section className="dl-block">
            <header className="dl-block-head">
              <h2>任务时间线</h2>
              <ArrowRight size={14} />
            </header>
            <TaskTimeline task={task} maxItems={7} />
          </section>
        </div>
      </div>

      {followUpOpen && <NewTaskModal project={project} onClose={() => setFollowUpOpen(false)} />}
    </div>
  );
}
