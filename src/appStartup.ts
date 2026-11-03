import { invokeCommand, TAURI_COMMANDS } from "./api";
import { hasTauriRuntime } from "./hooks/runtime";

export interface StartupTransport {
  context(): Promise<number>;
  report(generation: number, status: "ready" | "failed"): Promise<boolean>;
}
/** One document's generation is captured before loading React. Never fetch a new generation for a late acknowledgement. */
export function createStartupHandshake(transport: StartupTransport) {
  const generation = transport.context();
  let acknowledgement: Promise<boolean> | undefined;
  return {
    prepared: generation.then(() => undefined),
    ready: () => (acknowledgement ??= generation.then((value) => transport.report(value, "ready"))),
    failed: () => generation.then((value) => transport.report(value, "failed")),
  };
}
let handshake: ReturnType<typeof createStartupHandshake> | undefined;
export async function prepareStartup() {
  if (!hasTauriRuntime()) return;
  handshake = createStartupHandshake({
    context: () => invokeCommand<number>(TAURI_COMMANDS.webviewStartupContext),
    report: (generation, status) => invokeCommand<boolean>(TAURI_COMMANDS.reportWebviewStartup, { generation, status }),
  });
  await handshake.prepared;
}
/** Called only once the application shell commits. Tests mounting App directly need no native handshake. */
export function acknowledgeStartup() {
  if (!handshake) return;
  void handshake
    .ready()
    .then((accepted) => {
      if (accepted) document.documentElement.dataset.loomReady = "true";
    })
    .catch(() => {
      /* Native startup timer remains responsible when IPC is unavailable. */
    });
}
export function startupFailed() {
  const status = document.getElementById("loom-startup-status");
  if (status) {
    status.textContent = "Loom could not open. Please retry. / Loom 未能打开，请重试。";
    status.setAttribute("role", "alert");
  }
  const button = document.getElementById("loom-startup-retry");
  if (button) button.hidden = false;
  if (handshake) void handshake.failed().catch(() => {});
}
