import { createSignal, For, Show } from "solid-js";
import { LogOut, Users } from "lucide-solid";

import { useStore } from "../App";
import { MessageList } from "./MessageList";
import "../styles/ChatPane.css";

export default function ChatPane() {
  const store = useStore();
  const [input, setInput] = createSignal("");
  const [membersOpen, setMembersOpen] = createSignal(false);

  async function sendMessage() {
    const content = input().trim();
    if (!content) return;
    setInput("");
    await store.sendMessage(content);
  }

  async function toggleMembers() {
    const opening = !membersOpen();
    if (opening) {
      const session = store.currentSession();
      if (session) await store.fetchMembers(session.session_id);
    }
    setMembersOpen(opening);
  }

  const messages = () =>
    store.activeChannel() === "global"
      ? store.globalMessages()
      : store.sessionMessages();

  const channelName = () => {
    if (store.activeChannel() === "global") return "#global";
    const s = store.currentSession();
    return s ? `#${s.session_name}` : "#global";
  };

  // Non-null only when actively in a session channel
  const sessionCtx = () =>
    store.activeChannel() === "session" ? store.currentSession() : null;

  return (
    <div class="chat-pane">
      <Show
        when={store.connectionStatus() === "connected"}
        fallback={
          <div class="chat-pane-empty">
            <p>Select a server to start chatting</p>
          </div>
        }
      >
        <div class="chat-header">
          <div class="chat-header-left">
            <span class="chat-channel-name">{channelName()}</span>
            <Show when={sessionCtx()}>
              {(session) => (
                <span
                  class={`session-badge ${session().is_public ? "badge-public" : "badge-private"}`}
                >
                  {session().is_public ? "Public" : "Private"}
                </span>
              )}
            </Show>
          </div>
          <div class="chat-header-right">
            <Show when={store.activeChannel() === "global"}>
              <button
                class={`btn btn-secondary btn-icon${store.showMemberList() ? " btn-active" : ""}`}
                onClick={() => store.setShowMemberList(!store.showMemberList())}
                title="Toggle online list"
              >
                <Users size={16} stroke-width={1.75} />
                {store.globalMembers().length > 0
                  ? store.globalMembers().length
                  : ""}
              </button>
            </Show>
            <Show when={sessionCtx()}>
              <div class="members-wrapper">
                <button
                  class={`btn btn-secondary btn-icon${membersOpen() ? " btn-active" : ""}`}
                  onClick={toggleMembers}
                  title="Members"
                >
                  <Users size={16} stroke-width={1.75} />
                  {store.members().length > 0 ? store.members().length : ""}
                </button>
                <Show when={membersOpen()}>
                  <div class="members-dropdown">
                    <For
                      each={store.members()}
                      fallback={<p class="members-empty">No members</p>}
                    >
                      {(m) => {
                        const isSelf = () => m === store.username();
                        const connState = () =>
                          isSelf() ? null : (store.peerStates().get(m) ?? null);
                        const dotClass = () => {
                          switch (connState()) {
                            case "checking":
                              return "peer-dot peer-dot-checking";
                            case "connected":
                              return "peer-dot peer-dot-connected";
                            case "open":
                              return "peer-dot peer-dot-open";
                            case "failed":
                            case "disconnected":
                            case "closed":
                              return "peer-dot peer-dot-closed";
                            default:
                              return "peer-dot peer-dot-unknown";
                          }
                        };
                        return (
                          <div
                            class={`member-item ${isSelf() ? "member-self" : ""}`}
                          >
                            <span
                              class={dotClass()}
                              title={connState() ?? "not yet connected"}
                            />
                            {m}
                            {isSelf() ? " (you)" : ""}
                          </div>
                        );
                      }}
                    </For>
                  </div>
                </Show>
              </div>
              <button
                class="btn btn-secondary btn-icon"
                onClick={() => store.leaveSession()}
                title="Leave session"
              >
                <LogOut size={16} stroke-width={1.75} />
              </button>
            </Show>
          </div>
        </div>

        <MessageList messages={messages()} username={store.username()} />

        <div class="chat-input">
          <input
            type="text"
            placeholder={`Message ${channelName()}`}
            value={input()}
            onInput={(e) => setInput(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && sendMessage()}
          />
        </div>
      </Show>
    </div>
  );
}
