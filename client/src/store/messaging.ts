import { createSignal } from "solid-js";

import type { ActiveChannel, ChatMessage } from "../types";
import type { StreamHandle } from "./streams";

export function createMessagingSlice() {
  const [globalMessages, setGlobalMessages] = createSignal<ChatMessage[]>([]);
  const [sessionMessages, setSessionMessages] = createSignal<ChatMessage[]>([]);
  const [activeChannel, setActiveChannel] =
    createSignal<ActiveChannel>("global");

  const globalHandle: StreamHandle = { unlisten: null };
  const sessionHandle: StreamHandle = { unlisten: null };

  return {
    globalMessages,
    setGlobalMessages,
    sessionMessages,
    setSessionMessages,
    activeChannel,
    setActiveChannel,
    globalHandle,
    sessionHandle,
  };
}
