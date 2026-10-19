import type { CoreReply, CoreRequest, SessionEnvelope, SessionView } from "../src/workbench";

// Compile the JSON wire form, including camelCase command fields, serde unions,
// nullable metadata and number (not bigint) sequence values.
const request = {
  protocolVersion: 1,
  requestId: "client-1",
  command: {
    method: "patch_session",
    params: {
      key: { workspaceId: "workspace", sessionId: "session" },
      expectedSeq: 4,
      patch: {
        title: { op: "set", value: "New title" },
        statusId: null, archived: null, flagged: null, labels: null,
        permissionMode: null, connection: null, enabledSources: null, notes: null,
      },
    },
  },
} satisfies CoreRequest;
const event = {
  schemaVersion: 1, seq: 2, stream: "session/workspace/session", timestampMs: 1,
  previousChecksum: "previous", checksum: "current",
  payload: { kind: "turn_cancel_requested", turnId: "turn" },
} satisfies SessionEnvelope;
function session(reply: CoreReply): SessionView | undefined {
  if (reply.status === "failure") throw new Error(reply.error.message);
  if (reply.result.type === "session") return reply.result.data;
  return undefined;
}
void request; void event; void session;
