import { Folder, Bot } from "lucide-react";
import "./Sidebar.css";

export function Navigation() {
  return (
    <div className="sidebar-nav-container">
      <nav className="sidebar-nav-section">
        <h3 className="nav-heading">PROJECTS</h3>
        <ul className="nav-list">
          <li>
            <a href="#projects" className="nav-item active">
              <Folder size={16} className="nav-icon active-icon" />
              <span>sample-project</span>
            </a>
          </li>
          <li>
            <a href="#webapp" className="nav-item">
              <Folder size={16} className="nav-icon" />
              <span>my-web-app</span>
            </a>
          </li>
        </ul>
      </nav>

      <nav className="sidebar-nav-section">
        <h3 className="nav-heading">AGENTS</h3>
        <ul className="nav-list">
          <li>
            <a href="#codex" className="nav-item">
              <Bot size={16} className="nav-icon" />
              <span>Codex</span>
            </a>
          </li>
          <li>
            <a href="#claude" className="nav-item">
              <Bot size={16} className="nav-icon" />
              <span>Claude Code</span>
            </a>
          </li>
        </ul>
      </nav>
    </div>
  );
}
