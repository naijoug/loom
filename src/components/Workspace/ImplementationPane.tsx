import { Bot } from "lucide-react";
import "./Workspace.css";

export function ImplementationPane() {
  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">IMPLEMENTATION</span>
      </div>
      
      <div className="pane-content implementation-content">
        <div className="plan-card">
          <div className="plan-title">
            Current Plan
          </div>
          <div className="plan-desc">
            Implement the user authentication flow using the existing database schema. Ensure tests are added.
          </div>
        </div>

        <div className="agent-msg-container">
          <div className="agent-msg-header">
            <Bot size={16} className="agent-icon" />
            <span className="agent-name">Codex (Primary)</span>
          </div>
          <div className="agent-msg-box">
            I have added the login endpoint to src/api/auth.ts and created the corresponding unit tests.
          </div>
        </div>
      </div>
    </div>
  );
}
