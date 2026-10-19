import { Clock3 } from "lucide-react";
import type { Task } from "../../domain";
import { buildTaskTimelineRows } from "../../utils/taskTimeline";

interface TaskTimelineProps {
  task: Task;
  maxItems?: number;
}

export function TaskTimeline({ task, maxItems = 8 }: TaskTimelineProps) {
  const rows = buildTaskTimelineRows(task, maxItems);

  if (rows.length === 0) {
    return <p className="task-timeline-empty">尚未记录任务时间线。</p>;
  }

  return (
    <ol className="task-timeline">
      {rows.map((row) => (
        <li key={row.id}>
          <div className="task-timeline-icon">
            <Clock3 size={13} />
          </div>
          <div className="task-timeline-body">
            <div className="task-timeline-title">
              <span>{row.stage}</span>
              <em>{row.kind}</em>
            </div>
            <p>{row.summary}</p>
            {row.detail && <code>{row.detail}</code>}
          </div>
        </li>
      ))}
    </ol>
  );
}
