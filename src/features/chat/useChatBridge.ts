import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ChatEvent, ChatEventPage, ChatRuntimeError, ChatSession } from "../../domain/chat";
import { chatGet, chatReadEvents } from "../../api/chatClient";
import { listenToEvent, TAURI_EVENTS } from "../../api";
import { mockChatStore } from "./mockStore";
import { ChatProjection } from "./state/ChatProjection";

export interface ChatBridgeApi {
  get: (project: string, session: string) => Promise<ChatSession>;
  readEvents: (project: string, session: string, after: number) => Promise<ChatEventPage>;
  listen: (handler: (event: ChatEvent) => void) => Promise<() => void>;
  listenErrors: (handler: (event: ChatRuntimeError) => void) => Promise<() => void>;
}
const defaultApi: ChatBridgeApi = {
  get: chatGet,
  readEvents: chatReadEvents,
  listen: (handler) => listenToEvent(TAURI_EVENTS.chatEvent, handler),
  listenErrors: (handler) => listenToEvent(TAURI_EVENTS.chatError, handler),
};

/** One production source of truth for snapshot, ordered events and recovery. */
export function useChatBridge(
  projectPath: string | null,
  sessionId: string | null,
  useBackend: boolean,
  onError: (message: string) => void,
  onChanged: () => void,
  api: ChatBridgeApi = defaultApi,
) {
  const projection = useMemo(() => new ChatProjection(projectPath, sessionId), [projectPath, sessionId]);
  const current = useRef(projection);
  current.current = projection;
  const callbacks = useRef({ onError, onChanged });
  callbacks.current = { onError, onChanged };
  const [rendered, setRendered] = useState<ChatSession | null>(null);
  const [readyProject, setReadyProject] = useState<string | null>(null);
  const recovering = useRef(new WeakSet<ChatProjection>());
  const listenReady = !useBackend || (projectPath !== null && readyProject === projectPath);

  const paint = useCallback((target: ChatProjection) => {
    if (current.current === target) setRendered(target.session);
  }, []);
  const load = useCallback(async (target: ChatProjection) => {
    if (!target.projectPath || !target.sessionId) return;
    const snapshot = useBackend
      ? await api.get(target.projectPath, target.sessionId)
      : mockChatStore.get(target.sessionId) ?? null;
    if (current.current !== target) return;
    target.acceptSnapshot(snapshot);
    paint(target);
  }, [api, useBackend, paint]);

  const recover = useCallback(async (target = current.current) => {
    if (!useBackend || !target.projectPath || !target.sessionId || recovering.current.has(target)) return;
    recovering.current.add(target);
    try {
      if (!target.session || target.needsSnapshot) await load(target);
      for (let page = 0; page < 50 && current.current === target && target.session; page += 1) {
        const batch = await api.readEvents(target.projectPath, target.sessionId, target.lastSeq);
        if (current.current !== target) return;
        for (const event of batch.events) target.receive(event);
        paint(target);
        if (batch.events.some((event) => event.kind !== "stream")) callbacks.current.onChanged();
        if (target.needsSnapshot) { await load(target); return; }
        if (!batch.hasMore) return;
      }
      if (current.current === target) await load(target);
    } catch (error) {
      if (current.current === target) callbacks.current.onError(error instanceof Error ? error.message : String(error));
    } finally {
      recovering.current.delete(target);
    }
  }, [api, useBackend, load, paint]);

  useEffect(() => {
    if (!useBackend || !projectPath) return;
    let disposed = false;
    const cleanup: Array<() => void> = [];
    setReadyProject(null);
    void (async () => {
      const unsubscribe = await api.listen((event) => {
        if (disposed || event.projectKey !== projectPath) return;
        const target = current.current;
        if (target.projectPath !== projectPath) return;
        target.receive(event);
        paint(target);
        if (target.needsReplay) void recover(target);
        if (event.kind !== "stream") callbacks.current.onChanged();
      });
      if (disposed) { unsubscribe(); return; }
      cleanup.push(unsubscribe);
      const unsubscribeErrors = await api.listenErrors((event) => {
        const target = current.current;
        if (disposed || event.projectKey !== target.projectPath || event.sessionId !== target.sessionId) return;
        if (target.session?.activeTurnId === event.turnId) callbacks.current.onError(event.errorSummary);
      });
      if (disposed) { unsubscribeErrors(); return; }
      cleanup.push(unsubscribeErrors);
      setReadyProject(projectPath);
    })().catch((error) => {
      if (!disposed) callbacks.current.onError(error instanceof Error ? error.message : String(error));
    });
    return () => { disposed = true; for (const unsubscribe of cleanup) unsubscribe(); };
  }, [api, projectPath, useBackend, paint, recover]);

  useEffect(() => {
    setRendered(null);
    if (!listenReady || !sessionId || !projectPath) return;
    void load(projection).then(() => {
      if (current.current === projection && projection.needsReplay) void recover(projection);
    }).catch((error) => {
      if (current.current === projection) callbacks.current.onError(error instanceof Error ? error.message : String(error));
    });
  }, [projection, listenReady, projectPath, sessionId, load, recover]);

  const session = rendered?.projectPath === projectPath && rendered.id === sessionId ? rendered : null;
  useEffect(() => {
    if (!useBackend || !listenReady || !sessionId) return;
    const sync = () => { void recover(); };
    window.addEventListener("focus", sync);
    const timer = session?.turnStatus === "streaming" ? window.setInterval(sync, 1500) : null;
    return () => {
      window.removeEventListener("focus", sync);
      if (timer !== null) window.clearInterval(timer);
    };
  }, [useBackend, listenReady, sessionId, session?.turnStatus, recover]);

  const setSession = useCallback((snapshot: ChatSession | null) => {
    if (!snapshot) { setRendered(null); return; }
    const target = current.current;
    if (target.acceptSnapshot(snapshot)) paint(target);
  }, [paint]);
  return { session, setSession, listenReady, refresh: recover };
}
