import { chatSend, type ChatSendArgs, type ChatSendResult } from "../../../api/chatClient";

type Submission = Omit<ChatSendArgs, "clientRequestId">;

/** Keep ambiguous sends retryable for the lifetime of the renderer, including
 * Chat page remounts. No automatic retry of an agent execution is performed. */
export class ChatSendController {
  private attempts = new Map<string, { id: string; pending?: Promise<ChatSendResult> }>();

  constructor(
    private transport: (args: ChatSendArgs) => Promise<ChatSendResult> = chatSend,
    private newId: () => string = () => crypto.randomUUID(),
  ) {}

  send(input: Submission): Promise<ChatSendResult> {
    const key = JSON.stringify([input.projectPath, input.sessionId, input.text, input.permissionMode ?? null]);
    const attempt = this.attempts.get(key) ?? { id: this.newId() };
    if (attempt.pending) return attempt.pending;
    this.attempts.set(key, attempt);
    attempt.pending = Promise.resolve().then(() => this.transport({ ...input, clientRequestId: attempt.id }))
      .then((result) => {
        if (this.attempts.get(key) === attempt) this.attempts.delete(key);
        return result;
      }, (error: unknown) => {
        // Keep the same identity: an IPC failure is not proof of non-acceptance.
        attempt.pending = undefined;
        throw error;
      });
    return attempt.pending;
  }
}

export const chatSendController = new ChatSendController();
