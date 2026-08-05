import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TauriCommand, TauriEvent } from "./contract";

export function invokeCommand<T>(
  command: TauriCommand,
  args?: Record<string, unknown>,
): Promise<T> {
  return invoke<T>(command, args);
}

export function listenToEvent<T>(
  event: TauriEvent,
  onPayload: (payload: T) => void,
): Promise<UnlistenFn> {
  return listen<T>(event, (message) => onPayload(message.payload));
}
