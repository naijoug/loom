// Bounded MCP stdio client for the opt-in Research R0 probe. Not a product runtime.
import { spawn } from "node:child_process";

export class ProbeMcp {
  constructor(program, args, { cwd, env = {}, maxFrameBytes = 64 * 1024 * 1024 } = {}) {
    this.pending = new Map();
    this.sequence = 0;
    this.notifications = 0;
    this.buffer = Buffer.alloc(0);
    this.stderrBytes = 0;
    this.maxFrameBytes = maxFrameBytes;
    this.child = spawn(program, args, {
      cwd,
      env: { ...process.env, ...env },
      detached: true,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.exit = new Promise((resolve) =>
      this.child.once("close", (code, signal) => {
        this.ended = true;
        resolve({ code, signal });
      }),
    );
    this.child.on("error", (error) => this.fail(error));
    this.child.on("exit", (code, signal) => this.fail(new Error(`MCP exited (${code ?? signal})`)));
    this.child.stdin.on("error", (error) => this.fail(error));
    this.child.stderr.on("data", (bytes) => {
      this.stderrBytes += bytes.length;
    });
    this.child.stdout.on("data", (bytes) => {
      this.buffer = Buffer.concat([this.buffer, bytes]);
      let newline;
      while ((newline = this.buffer.indexOf(10)) >= 0) {
        if (newline > this.maxFrameBytes) return this.fail(new Error("MCP frame limit exceeded"));
        const line = this.buffer.subarray(0, newline);
        this.buffer = this.buffer.subarray(newline + 1);
        if (!line.length) continue;
        let message;
        try {
          message = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(line));
        } catch {
          return this.fail(new Error("Invalid MCP JSON/UTF-8"));
        }
        if (message.id == null) {
          this.notifications++;
          continue;
        }
        const request = this.pending.get(message.id);
        if (!request) continue;
        this.pending.delete(message.id);
        clearTimeout(request.timer);
        if (message.error)
          request.reject(new Error(`MCP error ${message.error.code}: ${String(message.error.message).slice(0, 160)}`));
        else request.resolve(message.result);
      }
      if (this.buffer.length > this.maxFrameBytes) this.fail(new Error("MCP frame limit exceeded"));
    });
  }
  fail(error) {
    this.failure ??= error;
    for (const item of this.pending.values()) {
      clearTimeout(item.timer);
      item.reject(error);
    }
    this.pending.clear();
  }
  notify(method, params = {}) {
    this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  }
  request(method, params = {}, timeoutMs = 30_000) {
    if (this.failure) return Promise.reject(this.failure);
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        this.notify("notifications/cancelled", { requestId: id, reason: "probe deadline" });
        reject(new Error(`MCP deadline: ${method}`));
      }, timeoutMs);
      this.pending.set(id, { resolve, reject, timer });
      this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  }
  call(name, args, timeoutMs = 120_000) {
    return this.request("tools/call", { name, arguments: args }, timeoutMs);
  }
  async close() {
    this.fail(new Error("Probe closed"));
    const kill = (signal) => {
      try {
        process.kill(-this.child.pid, signal);
      } catch (error) {
        if (error.code !== "ESRCH") throw error;
      }
    };
    if (this.ended) return this.exit;
    kill("SIGTERM");
    const timer = setTimeout(() => kill("SIGKILL"), 3000);
    try {
      return await this.exit;
    } finally {
      clearTimeout(timer);
    }
  }
}

export function structured(result) {
  const checked = (value) => {
    if (result?.isError || value?.error)
      throw new Error(`REA tool failure: ${value?.error?.code ?? "unknown"} (${value?.error?.category ?? "unknown"})`);
    return value;
  };
  if (result?.structuredContent) return checked(result.structuredContent);
  for (const block of result?.content ?? []) {
    if (block.type === "text") {
      let value;
      try {
        value = JSON.parse(block.text);
      } catch {
        continue;
      }
      return checked(value);
    }
  }
  throw new Error("REA returned no structured record");
}
