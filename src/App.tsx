import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { HealthCheckResult, SpikeLogEvent, SpikeRun, SpikeStopResult, WorkflowStage } from "./domain";
import "./App.css";

const stages: WorkflowStage[] = [
  {
    id: "planning",
    title: "Requirements",
    status: "ready",
    outcome: "Multi-agent planning, conflicts, and risk notes",
  },
  {
    id: "implementation",
    title: "Implementation",
    status: "pending",
    outcome: "Primary agent execution with reviewer checkpoints",
  },
  {
    id: "debugging",
    title: "Debugging",
    status: "pending",
    outcome: "Command runs, live logs, and error summaries",
  },
  {
    id: "summary",
    title: "Summary",
    status: "pending",
    outcome: "Changed files, verification evidence, residual risk",
  },
];

function formatTimestamp(timestampMs: number): string {
  return new Date(timestampMs).toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function App() {
  const [health, setHealth] = useState<HealthCheckResult | null>(null);
  const [healthError, setHealthError] = useState<string | null>(null);
  const [spikeRun, setSpikeRun] = useState<SpikeRun | null>(null);
  const [spikeLogs, setSpikeLogs] = useState<SpikeLogEvent[]>([]);
  const [spikeError, setSpikeError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<SpikeLogEvent>("loom://spike-log", (event) => {
      setSpikeLogs((current) => [...current.slice(-119), event.payload]);
    });

    return () => {
      void unlisten.then((remove) => remove());
    };
  }, []);

  const visibleLogText = useMemo(() => {
    if (spikeLogs.length === 0) {
      return "awaiting process logs...";
    }

    return spikeLogs
      .map((entry) => {
        const time = formatTimestamp(entry.timestampMs);
        return `${time} ${entry.stream.toUpperCase()} ${entry.line}`;
      })
      .join("\n");
  }, [spikeLogs]);

  async function runHealthCheck() {
    setHealthError(null);

    try {
      setHealth(await invoke<HealthCheckResult>("health_check"));
    } catch (error) {
      setHealthError(error instanceof Error ? error.message : String(error));
    }
  }

  async function startSpikeRun() {
    setSpikeError(null);
    setSpikeLogs([]);

    try {
      setSpikeRun(await invoke<SpikeRun>("start_spike_run"));
    } catch (error) {
      setSpikeError(error instanceof Error ? error.message : String(error));
    }
  }

  async function stopSpikeRun() {
    if (!spikeRun) {
      return;
    }

    setSpikeError(null);

    try {
      const result = await invoke<SpikeStopResult>("stop_spike_run", {
        runId: spikeRun.runId,
      });
      setSpikeRun(null);
      setSpikeLogs((current) => [
        ...current.slice(-119),
        {
          runId: result.runId,
          stream: "stdout",
          line: `stopped=${result.stopped} exitCode=${result.exitCode ?? "signal"}`,
          timestampMs: Date.now(),
        },
      ]);
    } catch (error) {
      setSpikeError(error instanceof Error ? error.message : String(error));
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar" aria-label="Loom workspace">
        <div className="brand-block">
          <span className="brand-mark">LM</span>
          <div>
            <p className="eyebrow">Loom / MVP</p>
            <p className="brand-title">Agent Workbench</p>
          </div>
        </div>

        <section className="runtime-card" aria-label="runtime profile">
          <div>
            <span className="label">Runtime</span>
            <strong>Tauri 2 / React 18</strong>
          </div>
          <div>
            <span className="label">Mode</span>
            <strong>Local orchestrator</strong>
          </div>
        </section>

        <nav className="stage-nav" aria-label="workflow stages">
          {stages.map((stage, index) => (
            <a className={`stage-link stage-link--${stage.status}`} href={`#${stage.id}`} key={stage.id}>
              <span className="stage-index">{index + 1}</span>
              <span>
                <strong>{stage.title}</strong>
                <small>{stage.outcome}</small>
              </span>
            </a>
          ))}
        </nav>
      </aside>

      <section className="workspace">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">M1 / Bridge Online</p>
            <p className="section-title">Workflow shell and Rust command bridge</p>
          </div>
          <button className="primary-action" onClick={runHealthCheck} type="button">
            Probe Backend
          </button>
        </header>

        <section className="status-strip" aria-label="backend health">
          <div>
            <span className="label">Backend</span>
            <strong>{health?.status ?? "not checked"}</strong>
          </div>
          <div>
            <span className="label">Runtime</span>
            <strong>{health ? `${health.app} ${health.version}` : "Tauri"}</strong>
          </div>
          <div>
            <span className="label">Last Probe</span>
            <strong>{health ? formatTimestamp(health.timestampMs) : "idle"}</strong>
          </div>
        </section>
        {healthError ? <p className="error-line">{healthError}</p> : null}

        <section className="stage-grid" aria-label="workflow overview">
          {stages.map((stage) => (
            <article className="stage-card" id={stage.id} key={stage.id}>
              <div className="stage-card__header">
                <h3>{stage.title}</h3>
                <span className={`status-pill status-pill--${stage.status}`}>{stage.status}</span>
              </div>
              <p>{stage.outcome}</p>
            </article>
          ))}
        </section>

        <section className="spike-panel" aria-label="process spike">
          <div className="spike-panel__header">
            <div>
              <p className="eyebrow">Process Spike</p>
              <p className="panel-title">tokio::process live stream</p>
            </div>
            <div className="button-row">
              <button disabled={Boolean(spikeRun)} onClick={startSpikeRun} type="button">
                Start
              </button>
              <button disabled={!spikeRun} onClick={stopSpikeRun} type="button">
                Stop
              </button>
            </div>
          </div>

          <dl className="run-metadata">
            <div>
              <dt>Run ID</dt>
              <dd>{spikeRun?.runId ?? "no active process"}</dd>
            </div>
            <div>
              <dt>PID</dt>
              <dd>{spikeRun?.pid ?? "-"}</dd>
            </div>
            <div>
              <dt>Working Directory</dt>
              <dd>{spikeRun?.cwd ?? "-"}</dd>
            </div>
          </dl>

          {spikeError ? <p className="error-line">{spikeError}</p> : null}
          <pre className="log-view" aria-live="polite">
            {visibleLogText}
          </pre>
        </section>
      </section>
    </main>
  );
}

export default App;
