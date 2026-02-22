import { createEffect, For, Show } from "solid-js";

import type { ChatMessage } from "../types";

interface MessageListProps {
  messages: ChatMessage[];
  username: string;
}

export function MessageList(props: MessageListProps) {
  let endRef: HTMLDivElement | undefined;

  createEffect(() => {
    props.messages;
    endRef?.scrollIntoView({ behavior: "smooth" });
  });

  return (
    <div class="messages">
      <For each={props.messages}>
        {(msg) => (
          <Show
            when={!msg.system}
            fallback={<div class="message-system">{msg.content}</div>}
          >
            <div
              class={`message ${msg.username === props.username ? "own" : ""}`}
            >
              <Show when={msg.username !== props.username}>
                <div class="message-author">{msg.username}</div>
              </Show>
              <div class="message-bubble">{msg.content}</div>
            </div>
          </Show>
        )}
      </For>
      <div ref={(el) => (endRef = el)} />
    </div>
  );
}
