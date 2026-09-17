import { useCallback, useReducer } from "react";
import type { ChatSession, ChatSessionSummary } from "../../domain";
import {
  chatStoreBumpGeneration,
  chatStoreDraftFor,
  chatStoreSelectProject,
  chatStoreSelectSession,
  chatStoreSetDraft,
  chatStoreSetListenReady,
  chatStoreSetSession,
  chatStoreSetSummaries,
  createChatStoreSnapshot,
  type ChatStoreSnapshot,
} from "./state/chatStore";

type Action =
  | { type: "project"; projectPath: string | null }
  | { type: "listenReady"; ready: boolean }
  | { type: "summaries"; summaries: ChatSessionSummary[]; generation: number }
  | { type: "select"; sessionId: string | null }
  | { type: "session"; session: ChatSession | null; generation: number; expectedSessionId?: string | null }
  | { type: "draft"; sessionId: string; draft: string }
  | { type: "bump" };

function reducer(state: ChatStoreSnapshot, action: Action): ChatStoreSnapshot {
  switch (action.type) {
    case "project":
      return chatStoreSelectProject(state, action.projectPath);
    case "listenReady":
      return chatStoreSetListenReady(state, action.ready);
    case "summaries":
      return chatStoreSetSummaries(state, action.summaries, action.generation);
    case "select":
      return chatStoreSelectSession(state, action.sessionId);
    case "session":
      return chatStoreSetSession(state, action.session, action.generation, action.expectedSessionId);
    case "draft":
      return chatStoreSetDraft(state, action.sessionId, action.draft);
    case "bump":
      return chatStoreBumpGeneration(state);
    default:
      return state;
  }
}

/** Isolates project/session/draft/generation for Chat; ChatPage can adopt incrementally. */
export function useChatBridge(projectPath: string | null) {
  const [state, dispatch] = useReducer(reducer, projectPath, createChatStoreSnapshot);

  const setProject = useCallback((next: string | null) => {
    dispatch({ type: "project", projectPath: next });
  }, []);

  const draft = chatStoreDraftFor(state, state.selectedSessionId);

  return { state, dispatch, setProject, draft };
}
