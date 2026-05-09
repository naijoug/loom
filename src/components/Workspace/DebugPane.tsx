import { Send } from "lucide-react";
import { Button } from "../common/Button";
import type { useTauriBridge } from "../../hooks/useTauriBridge";
import "./Workspace.css";

interface DebugPaneProps {
  bridge: ReturnType<typeof useTauriBridge>;
}

export function DebugPane({ bridge }: DebugPaneProps) {
  return (
    <div className="pane-container">
      <div className="pane-header">
        <span className="pane-header-title">DEBUG / LOGS</span>
        
        {/* Temporary controls for Tauri bridge testing */}
        <div className="debug-controls">
          <button onClick={bridge.runHealthCheck} className="debug-btn">Probe Backend</button>
          <button onClick={bridge.startSpikeRun} disabled={!!bridge.spikeRun} className="debug-btn">Start Spike</button>
          <button onClick={bridge.stopSpikeRun} disabled={!bridge.spikeRun} className="debug-btn">Stop Spike</button>
        </div>
      </div>

      <div className="terminal-container">
        {bridge.health && (
          <div className="terminal-line secondary-text">
            [System] Backend: {bridge.health.status} ({bridge.health.app} v{bridge.health.version})
          </div>
        )}
        {bridge.healthError && <div className="terminal-line error-text">{bridge.healthError}</div>}
        {bridge.spikeError && <div className="terminal-line error-text">{bridge.spikeError}</div>}
        
        <pre className="terminal-output">
          {bridge.visibleLogText}
        </pre>
        
        {/* Static mock data for visual completion */}
        <div className="terminal-line primary-text">$ pnpm run dev</div>
        <div className="terminal-line secondary-text">VITE v4.3.9 ready in 250 ms</div>
        <div className="terminal-line accent-text">  ➜  Local:   http://localhost:5173/</div>
        <div className="terminal-line error-text">ERROR: Failed to connect to database at api/auth.ts:42</div>
      </div>

      <div className="feedback-container">
        <div className="feedback-title">Manual Feedback</div>
        <div className="feedback-input-wrapper">
          <input 
            type="text" 
            className="feedback-input" 
            placeholder="The database connection is failing because the env var is missing."
          />
          <Button variant="primary" iconRight={<Send size={14} />} className="submit-btn">
            Submit
          </Button>
        </div>
      </div>
    </div>
  );
}
