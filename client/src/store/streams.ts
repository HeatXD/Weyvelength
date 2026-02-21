import { Setter } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";

import type { ChatMessage } from "../types";

export interface StreamHandle {
  unlisten: UnlistenFn | null;
}

export async function manageStream(
  handle: StreamHandle,
  stopCommand: string,
  eventName: string,
  setMessages: Setter<ChatMessage[]>,
  startCommand: string,
  startArgs: Record<string, unknown> = {},
): Promise<void> {
  if (handle.unlisten) {
    handle.unlisten();
    handle.unlisten = null;
  }
  await invoke(stopCommand);
  setMessages([]);
  handle.unlisten = await listen<ChatMessage>(eventName, (e) =>
    setMessages((prev) => [...prev, e.payload]),
  );
  await invoke(startCommand, startArgs);
}
