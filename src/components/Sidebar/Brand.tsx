import { Columns2 } from "lucide-react";
import "./Sidebar.css";

export function Brand() {
  return (
    <div className="sidebar-brand" data-tauri-drag-region>
      <div className="sb-logo">
        <span className="sb-mark" aria-hidden="true">
          <Columns2 size={18} />
        </span>
        <span className="sb-word">Loom</span>
      </div>
    </div>
  );
}
