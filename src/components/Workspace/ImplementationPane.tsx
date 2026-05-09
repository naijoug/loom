import { useEffect, useState, type FormEvent } from "react";
import { FolderGit2 } from "lucide-react";
import { useAgentBridge } from "../../hooks/useAgentBridge";
import { useTaskBridge } from "../../hooks/useTaskBridge";
import { useAppState } from "../../state/AppStateContext";
import { Button } from "../common/Button";
import "./Workspace.css";

export function ImplementationPane() {
  const { state } = useAppState();
  const { runDummyPlanning } = useAgentBridge();
  const { createTask, loadTasks } = useTaskBridge();
  const project = state.projects.current;
  const task = state.tasks.find((candidate) => candidate.id === state.app.selectedTaskId) ?? null;
  const dummyAgent = state.agents.find((agent) => agent.adapterType === "dummy" && agent.enabled);
  const [title, setTitle] = useState("");
  const [requirement, setRequirement] = useState("");

  useEffect(() => {
    if (project) {
      void loadTasks(project.path);
    }
  }, [loadTasks, project]);

  async function handleCreateTask(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();

    if (!project || !title.trim() || !requirement.trim()) {
      return;
    }

    const created = await createTask({
      projectPath: project.path,
      title: title.trim(),
      rawRequirement: requirement.trim(),
    });

    if (created) {
      setTitle("");
      setRequirement("");
    }
  }

  async function handleDummyPlanning() {
    if (!project || !task || !dummyAgent) {
      return;
    }

    await runDummyPlanning(project.path, task.id, dummyAgent.id);
  }

  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">IMPLEMENTATION</span>
      </div>
      
      <div className="pane-content implementation-content">
        <div className="plan-card">
          <div className="plan-title">
            {project ? project.name : "Project"}
          </div>
          <div className="plan-desc">
            {project
              ? project.path
              : "Add a local project path from the sidebar to start the development loop."}
          </div>
        </div>

        {project && (
          <div className="agent-msg-container">
            <div className="agent-msg-header">
              <FolderGit2 size={16} className="agent-icon" />
              <span className="agent-name">Project analysis</span>
            </div>
            <div className="agent-msg-box">
              <div>Stacks: {project.detectedStacks.join(", ") || "unknown"}</div>
              <div>Suggested commands: {project.suggestedCommands.join(", ") || "none"}</div>
              <div>
                Git: {project.isGitRepository ? project.gitBranch ?? "repository" : "not detected"}
                {project.hasUncommittedChanges ? " with local changes" : ""}
              </div>
              <div>Store: {project.loomDirReady ? `.loom schema v${project.schemaVersion}` : "not ready"}</div>
            </div>
          </div>
        )}

        {project && (
          <form className="task-form" onSubmit={handleCreateTask}>
            <div className="plan-title">New task</div>
            <input
              className="workspace-input"
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Task title"
            />
            <textarea
              className="workspace-textarea"
              value={requirement}
              onChange={(event) => setRequirement(event.target.value)}
              placeholder="Describe the development goal"
            />
            <Button
              type="submit"
              variant="primary"
              disabled={!title.trim() || !requirement.trim() || state.app.isLoadingTasks}
            >
              Create Task
            </Button>
            {state.app.taskError && <div className="inline-error">{state.app.taskError}</div>}
          </form>
        )}

        {task && (
          <div className="agent-msg-container">
            <div className="agent-msg-header">
              <FolderGit2 size={16} className="agent-icon" />
              <span className="agent-name">{task.title}</span>
            </div>
            <div className="agent-msg-box">
              <div>Status: {task.status}</div>
              <div>Requirement: {task.rawRequirement}</div>
              {task.finalPlan && <pre className="plan-preview">{task.finalPlan}</pre>}
            </div>
            <Button
              type="button"
              variant="ghost"
              disabled={!dummyAgent}
              onClick={handleDummyPlanning}
            >
              Run Dummy Planning
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
