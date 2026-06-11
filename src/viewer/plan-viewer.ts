import { invoke } from "@tauri-apps/api/core";

// Standalone entry for the plan viewer window opened by `open_plan_viewer`.
// It renders the safe HTML produced by `read_plan_html` inside a sandboxed
// iframe, mirroring the in-app preview's trust boundary.

const root = document.getElementById("plan-viewer-root");

function showMessage(text: string) {
  if (root) {
    root.innerHTML = "";
    const message = document.createElement("p");
    message.className = "plan-viewer-message";
    message.textContent = text;
    root.appendChild(message);
  }
}

async function main() {
  const params = new URLSearchParams(window.location.search);
  const projectPath = params.get("project");
  const mdPath = params.get("path");

  if (!root || !projectPath || !mdPath) {
    showMessage("Missing plan reference. Open this window from a planning task.");
    return;
  }

  try {
    const html = await invoke<string>("read_plan_html", { projectPath, mdPath });
    const fileName = mdPath.split("/").pop() ?? "Plan";
    document.title = `Loom Plan — ${fileName.replace(/\.md$/, "")}`;

    const iframe = document.createElement("iframe");
    iframe.setAttribute("sandbox", "");
    iframe.title = document.title;
    iframe.srcdoc = html;
    root.innerHTML = "";
    root.appendChild(iframe);
  } catch (error) {
    showMessage(`Failed to load plan: ${error instanceof Error ? error.message : String(error)}`);
  }
}

void main();
