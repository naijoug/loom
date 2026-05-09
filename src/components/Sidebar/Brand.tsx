import { Hexagon, Settings } from "lucide-react";
import { useAppState } from "../../state/AppStateContext";
import "./Sidebar.css";

export function Brand() {
  const { dispatch } = useAppState();

  return (
    <div className="sidebar-brand">
      <Hexagon size={24} className="brand-icon" />
      <span className="brand-title">LOOM</span>
      <button
        type="button"
        className="settings-icon-button"
        title="Settings"
        onClick={() => dispatch({ type: "app/viewSelected", view: "settings" })}
      >
        <Settings size={16} />
      </button>
    </div>
  );
}
