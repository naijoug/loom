import { ImplementationPane } from "./ImplementationPane";
import { DebugPane } from "./DebugPane";
import "./Workspace.css";

export function WorkspaceSplit() {
  return (
    <div className="workspace-split">
      <div className="workspace-pane pane-left">
        <ImplementationPane />
      </div>
      <div className="workspace-pane pane-right">
        <DebugPane />
      </div>
    </div>
  );
}
