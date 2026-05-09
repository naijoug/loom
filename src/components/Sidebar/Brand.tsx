import { Hexagon } from "lucide-react";
import "./Sidebar.css";

export function Brand() {
  return (
    <div className="sidebar-brand">
      <Hexagon size={24} className="brand-icon" />
      <span className="brand-title">LOOM</span>
    </div>
  );
}
