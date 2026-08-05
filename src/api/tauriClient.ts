import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TauriCommand, TauriEvent } from "./contract";

type InvokeTransport = (
  command: string,
  args?: Record<string, unknown>,
) => Promise<unknown>;

type ListenTransport = (
  event: string,
  handler: (message: { payload: unknown }) => void,
) => Promise<UnlistenFn>;

export function createTauriClient(
  invokeTransport: InvokeTransport,
  listenTransport: ListenTransport,
) {
  return {
    invoke<T>(command: TauriCommand, args?: Record<string, unknown>): Promise<T> {
      return invokeTransport(command, args) as Promise<T>;
    },
    listen<T>(event: TauriEvent, onPayload: (payload: T) => void): Promise<UnlistenFn> {
      return listenTransport(event, (message) => onPayload(message.payload as T));
    },
  };
}

const defaultClient = createTauriClient(invoke as InvokeTransport, listen as ListenTransport);

export function invokeCommand<T>(
  command: TauriCommand,
  args?: Record<string, unknown>,
): Promise<T> {
  return defaultClient.invoke<T>(command, args);
}

export function listenToEvent<T>(
  event: TauriEvent,
  onPayload: (payload: T) => void,
): Promise<UnlistenFn> {
  return defaultClient.listen<T>(event, onPayload);
}
