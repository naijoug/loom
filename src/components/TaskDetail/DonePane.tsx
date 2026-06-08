import { useMemo, useState } from "react";
import { CheckCircle2, FileText, GitBranch, Plus, ShieldCheck } from "lucide-react";
import type { ProjectSummary, Task } from "../../domain";
import { NewTaskModal } from "../Board";
import { Button } from "../common/Button";
import "./TaskDetail.css";

interface DonePaneProps {
  project: ProjectSummary;
  task: Task;
}

function latestValidation(task: Task) {
  return task.commandRuns
    .slice()
    .sort((left, right) => right.startedAtMs - left.startedAtMs)[0] ?? null;
}

export function DonePane({ project, task }: DonePaneProps) {
  const [followUpOpen, setFollowUpOpen] = useState(false);
  const validation = useMemo(() => latestValidation(task), [task]);
  const completedTodos = task.planTodos.filter((todo) => todo.status === "done");
  const recentEvents = task.events.slice(-6).reverse();

  return (
    <div className="done-pane">
      <div className="done-header">
        <div>
          <div className="done-kicker">Done</div>
          <h1>{task.title}</h1>
          <p>{project.name}</p>
        </div>
        <Button variant="primary" iconLeft={<Plus size={14} />} onClick={() => setFollowUpOpen(true)}>
          Start follow-up
        </Button>
      </div>

      <div className="done-grid">
        <section className="done-card">
          <div className="done-card-title">
            <CheckCircle2 size={16} />
            Completed items
          </div>
          {completedTodos.length > 0 ? (
            <ul className="done-list">
              {completedTodos.map((todo) => (
                <li key={todo.id}>{todo.title}</li>
              ))}
            </ul>
          ) : (
            <p>No implementation todos were recorded.</p>
          )}
        </section>

        <section className="done-card">
          <div className="done-card-title">
            <ShieldCheck size={16} />
            Verification evidence
          </div>
          {validation ? (
            <div className="done-evidence">
              <strong>{validation.command}</strong>
              <span>
                {validation.status}
                {typeof validation.exitCode === "number" ? ` · exit ${validation.exitCode}` : ""}
              </span>
            </div>
          ) : (
            <p>No command validation was recorded.</p>
          )}
        </section>

        <section className="done-card">
          <div className="done-card-title">
            <FileText size={16} />
            Plan reference
          </div>
          <p>{task.finalPlanPath ?? "No final plan path recorded."}</p>
        </section>

        <section className="done-card">
          <div className="done-card-title">
            <GitBranch size={16} />
            Timeline
          </div>
          {recentEvents.length > 0 ? (
            <ol className="done-timeline">
              {recentEvents.map((event) => (
                <li key={event.id}>
                  <span>{event.status.split("_").join(" ")}</span>
                  <p>{event.outputSummary ?? event.inputSummary ?? event.actor}</p>
                </li>
              ))}
            </ol>
          ) : (
            <p>No task events were recorded.</p>
          )}
        </section>
      </div>

      {followUpOpen && <NewTaskModal project={project} onClose={() => setFollowUpOpen(false)} />}
    </div>
  );
}
