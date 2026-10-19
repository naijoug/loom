import { useAppState } from "../../state/AppStateContext";
import { NewTaskModal } from "./NewTaskModal";

export function NewTaskModalHost() {
  const { state, dispatch } = useAppState();
  const project = state.projects.current;

  if (!state.app.isCreatingTask || !project) {
    return null;
  }

  return (
    <NewTaskModal
      project={project}
      onClose={() => dispatch({ type: "tasks/newClosed" })}
    />
  );
}
