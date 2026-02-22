import { For, Show } from "solid-js";

import { useStore } from "../App";
import "../styles/MemberListPanel.css";

export default function MemberListPanel() {
  const store = useStore();

  return (
    <Show
      when={
        store.showMemberList() &&
        store.connectionStatus() === "connected" &&
        store.activeChannel() === "global"
      }
    >
      <div class="member-list-panel">
        <div class="member-list-header">
          Online — {store.globalMembers().length}
        </div>
        <div class="member-list-body">
          <For
            each={store.globalMembers()}
            fallback={<p class="member-list-empty">No one online</p>}
          >
            {(member) => (
              <div
                class={`member-list-item ${member === store.username() ? "member-self" : ""}`}
              >
                {member}
                {member === store.username() ? " (you)" : ""}
              </div>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
}
