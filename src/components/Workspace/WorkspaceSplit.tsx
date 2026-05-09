import { ImplementationPane } from "./ImplementationPane";
import { DebugPane } from "./DebugPane";
import { useTauriBridge } from "../../hooks/useTauriBridge";
import "./Workspace.css";

export function WorkspaceSplit() {
  const bridge = useTauriBridge();

  return (
    <div className="workspace-split">
      <div className="workspace-pane pane-left">
        <ImplementationPane />
      </div>
      <div className="workspace-pane pane-right">
        <DebugPane bridge={bridge} />
      </div>
    </div>
  );
}
