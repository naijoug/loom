import type { ChatSession, ChatSessionSummary } from "../../../domain";

/**
 * Per-project / per-session UI projection for Chat.
 * Keeps draft + selection isolated so finishing turn A cannot clobber turn B,
 * and switching projects clears the previous project's selection.
 */
export type ChatStoreSnapshot = {
  projectPath: string | null;
  selectedSessionId: string | null;
  summaries: ChatSessionSummary[];
  session: ChatSession | null;
  draftBySessionId: Record<string, string>;
  /** Monotonic generation; ignore stale async results from older generations. */
  generation: number;
  listenReady: boolean;
};

export function createChatStoreSnapshot(projectPath: string | null = null): ChatStoreSnapshot {
  return {
    projectPath,
    selectedSessionId: null,
    summaries: [],
    session: null,
    draftBySessionId: {},
    generation: 0,
    listenReady: false,
  };
}

export function chatStoreSelectProject(
  state: ChatStoreSnapshot,
  projectPath: string | null,
): ChatStoreSnapshot {
  if (state.projectPath === projectPath) return state;
  return {
    ...createChatStoreSnapshot(projectPath),
    generation: state.generation + 1,
  };
}

export function chatStoreSetListenReady(
  state: ChatStoreSnapshot,
  listenReady: boolean,
): ChatStoreSnapshot {
  if (state.listenReady === listenReady) return state;
  return { ...state, listenReady };
}

export function chatStoreSetSummaries(
  state: ChatStoreSnapshot,
  summaries: ChatSessionSummary[],
  generation: number,
): ChatStoreSnapshot {
  if (generation !== state.generation) return state;
  return { ...state, summaries };
}

export function chatStoreSelectSession(
  state: ChatStoreSnapshot,
  sessionId: string | null,
): ChatStoreSnapshot {
  if (state.selectedSessionId === sessionId) return state;
  return { ...state, selectedSessionId: sessionId, session: null };
}

export function chatStoreSetSession(
  state: ChatStoreSnapshot,
  session: ChatSession | null,
  generation: number,
  expectedSessionId?: string | null,
): ChatStoreSnapshot {
  if (generation !== state.generation) return state;
  if (
    expectedSessionId != null &&
    state.selectedSessionId != null &&
    expectedSessionId !== state.selectedSessionId
  ) {
    return state;
  }
  return { ...state, session };
}

export function chatStoreSetDraft(
  state: ChatStoreSnapshot,
  sessionId: string,
  draft: string,
): ChatStoreSnapshot {
  if (state.draftBySessionId[sessionId] === draft) return state;
  return {
    ...state,
    draftBySessionId: { ...state.draftBySessionId, [sessionId]: draft },
  };
}

export function chatStoreDraftFor(
  state: ChatStoreSnapshot,
  sessionId: string | null,
): string {
  if (!sessionId) return "";
  return state.draftBySessionId[sessionId] ?? "";
}

export function chatStoreBumpGeneration(state: ChatStoreSnapshot): ChatStoreSnapshot {
  return { ...state, generation: state.generation + 1 };
}
