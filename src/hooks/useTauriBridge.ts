import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useMemo, useState } from "react";
import type { HealthCheckResult, SpikeLogEvent, SpikeRun, SpikeStopResult } from "../domain";

export function useTauriBridge() {
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
        const time = new Date(entry.timestampMs).toLocaleTimeString("en-US", {
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
        });
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

  return {
    health,
    healthError,
    spikeRun,
    spikeLogs,
    spikeError,
    visibleLogText,
    runHealthCheck,
    startSpikeRun,
    stopSpikeRun,
  };
}
