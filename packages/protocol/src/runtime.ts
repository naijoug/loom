// Generated from loom-runtime Rust types. Do not edit.
import type { Id, ConnectionConfig, JsonValue } from './workbench.js';

export const RUNTIME_PROTOCOL_VERSION = 2 as const;
export type Scope = { workspaceId: Id, sessionId: Id, turnId: Id, runId: Id, };
export type HistoryMessage = { role: string, text: string, };
export type ExternalTool = { name: string, description: string, inputSchema: JsonValue, };
export type RunConfig = { scope: Scope, connection: ConnectionConfig, model: string, thinking: string | null, cwd: string, runtimeDirectory: string, prompt: string, history: Array<HistoryMessage>, tools: Array<string>, externalTools: Array<ExternalTool>, maxTurns: number, };
export type WorkerEvent = { "kind": "ready" } | { "kind": "text", partId: Id, textKind: string, delta: string, } | { "kind": "tool_request", requestId: Id, toolCallId: Id, name: string, input: JsonValue, } | { "kind": "tool_result", toolCallId: Id, failed: boolean, output: JsonValue, } | { "kind": "usage", inputTokens: number, outputTokens: number, } | { "kind": "completed", cancelled: boolean, } | { "kind": "failed", code: string, message: string, };
export type Frame = { protocolVersion: number, scope: Scope, seq: number, event: WorkerEvent, };
export type ToolDecision = { requestId: Id, toolCallId: Id, allowed: boolean, output: JsonValue | null, };
export type Start = { protocolVersion: number, kind: "start", config: RunConfig, credential: string | null, };
export type DecisionFrame = { protocolVersion: number, kind: string, scope: Scope, decision: ToolDecision, };
export type CancelFrame = { protocolVersion: number, kind: string, scope: Scope, };
