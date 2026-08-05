import type { Task } from "../../domain";
import type { AppState } from "../model";

export function selectedTaskAfterLoad(tasks: Task[], currentTaskId: string | null) {
  return tasks.find((task) => task.id === currentTaskId) ?? tasks[tasks.length - 1] ?? null;
}

export function selectedTodoIdForTask(task: Task | null, currentTodoId: string | null) {
  if (!task) return null;
  return task.planTodos.some((todo) => todo.id === currentTodoId)
    ? currentTodoId
    : task.planTodos[0]?.id ?? null;
}

export function taskProjectPath(state: AppState, taskId: string) {
  const currentTask = state.tasks.find((task) => task.id === taskId);
  if (currentTask) return currentTask.projectPath;

  for (const tasks of Object.values(state.taskCache)) {
    const cachedTask = tasks.find((task) => task.id === taskId);
    if (cachedTask) return cachedTask.projectPath;
  }
  return null;
}

export function upsertTask(tasks: Task[], task: Task) {
  return [...tasks.filter((candidate) => candidate.id !== task.id), task].sort(
    (left, right) => left.createdAtMs - right.createdAtMs,
  );
}

export function replaceTask(tasks: Task[], taskId: string, update: (task: Task) => Task) {
  return tasks.map((task) => (task.id === taskId ? update(task) : task));
}

export function removeTask(tasks: Task[], taskId: string) {
  return tasks.filter((task) => task.id !== taskId);
}

export function cacheProjectTasks(
  cache: Record<string, Task[]>,
  projectPath: string,
  tasks: Task[],
) {
  return { ...cache, [projectPath]: tasks };
}

export function removeCachedProjectTasks(cache: Record<string, Task[]>, projectPath: string) {
  return Object.fromEntries(
    Object.entries(cache).filter(([cachedProjectPath]) => cachedProjectPath !== projectPath),
  );
}

export function mapCachedTasks(
  cache: Record<string, Task[]>,
  taskId: string,
  update: (task: Task) => Task,
) {
  return Object.fromEntries(
    Object.entries(cache).map(([projectPath, tasks]) => [
      projectPath,
      replaceTask(tasks, taskId, update),
    ]),
  );
}
