import type { ChatEvent, ChatSession } from "../../../domain/chat";

const MUTABLE_FIELDS = new Set([
  "title", "agentId", "permissionMode", "updatedAtMs", "resumeCommand",
  "activeTurnId", "turnStatus", "promotedTaskId", "status", "flagged",
  "sendReceipts",
  "resumeHandle",
  "turns",
]);

/** Projection of durable events, isolated to one selected project/session. */
export class ChatProjection {
  session: ChatSession | null = null;
  needsSnapshot = false;
  private pending = new Map<number, ChatEvent>();

  constructor(readonly projectPath: string | null, readonly sessionId: string | null) {}

  get lastSeq() { return this.session?.lastSeq ?? 0; }
  get needsReplay() { return this.needsSnapshot || this.pending.size > 0; }

  acceptSnapshot(snapshot: ChatSession | null): boolean {
    if (!snapshot || snapshot.projectPath !== this.projectPath || snapshot.id !== this.sessionId) return false;
    if (snapshot.schemaVersion === 2 && (snapshot.lastSeq ?? 0) < this.lastSeq) return false;
    this.session = snapshot;
    this.needsSnapshot = false;
    for (const seq of this.pending.keys()) if (seq <= this.lastSeq) this.pending.delete(seq);
    this.drain();
    return true;
  }

  receive(event: ChatEvent): boolean {
    if (event.projectKey !== this.projectPath || event.sessionId !== this.sessionId) return false;
    if (event.schemaVersion !== 2 || !Number.isSafeInteger(event.seq) || event.seq < 1) {
      this.needsSnapshot = true;
      return false;
    }
    if (event.seq <= this.lastSeq) return false;
    this.pending.set(event.seq, event);
    if (this.pending.size > 1_000) {
      this.pending.clear();
      this.needsSnapshot = true;
      return false;
    }
    if (!this.session && event.kind === "session_created" && event.seq === 1) {
      return this.acceptSnapshot(event.payload);
    }
    this.drain();
    return true;
  }

  private drain() {
    while (this.session && !this.needsSnapshot) {
      const next = this.pending.get(this.lastSeq + 1);
      if (!next) break;
      try {
        this.session = applyEvent(this.session, next);
        this.pending.delete(next.seq);
      } catch {
        this.needsSnapshot = true;
      }
    }
  }
}

function applyEvent(session: ChatSession, event: ChatEvent): ChatSession {
  let next: ChatSession;
  if (event.kind === "session_patch") {
    for (const key of Object.keys(event.payload.fields)) {
      if (!MUTABLE_FIELDS.has(key)) throw new Error("Unsupported chat field");
    }
    next = { ...session, ...event.payload.fields, messages: [...session.messages] };
    for (const [key, value] of Object.entries(event.payload.fields)) {
      if (value === null) Reflect.deleteProperty(next, key);
    }
    for (const changed of event.payload.changedMessages) {
      const index = next.messages.findIndex((message) => message.id === changed.id);
      if (index < 0) throw new Error("Missing chat message");
      next.messages[index] = changed;
    }
    for (const appended of event.payload.appendedMessages) {
      if (next.messages.some((message) => message.id === appended.id)) throw new Error("Duplicate chat message");
      next.messages.push(appended);
    }
  } else if (event.kind === "stream") {
    const stream = event.payload;
    if (stream.sessionId !== session.id || stream.turnId !== session.activeTurnId) throw new Error("Wrong chat turn");
    const index = session.messages.findIndex((message) => message.id === stream.messageId && message.role === "assistant");
    if (index < 0) throw new Error("Missing assistant message");
    const message = session.messages[index];
    const messages = [...session.messages];
    messages[index] = {
      ...message,
      content: stream.done ? message.content : message.content + stream.delta,
      parts: stream.part ? [...(message.parts ?? []), stream.part] : message.parts,
    };
    next = { ...session, messages, updatedAtMs: event.timestampMs };
  } else {
    throw new Error("Duplicate chat creation event");
  }
  return { ...next, lastSeq: event.seq, revision: event.seq };
}
