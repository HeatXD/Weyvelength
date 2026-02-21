import { For } from "solid-js";
import { Plus, X } from "lucide-solid";

import { useStore } from "../App";
import "../styles/ServerSidebar.css";

export default function ServerSidebar() {
  const store = useStore();

  return (
    <div class="server-sidebar">
      <For each={store.servers()}>
        {(server) => (
          <div class="server-icon-wrapper">
            <button
              class={`server-icon ${store.activeServerId() === server.id ? "active" : ""}`}
              onClick={() =>
                store.activeServerId() === server.id
                  ? store.disconnect()
                  : store.connectToServer(server.id)
              }
              title={store.activeServerId() === server.id ? `Disconnect from ${server.displayName}` : server.displayName}
            >
              {server.displayName.slice(0, 2).toUpperCase()}
            </button>
            <button
              class="server-remove"
              onClick={(e) => {
                e.stopPropagation();
                store.removeServer(server.id);
              }}
              title={`Remove ${server.displayName}`}
            >
              <X size={8} stroke-width={2.5} />
            </button>
          </div>
        )}
      </For>
      <button
        class="server-icon server-add"
        onClick={() => store.setShowAddServer(true)}
        title="Add Server"
      >
        <Plus size={20} stroke-width={1.75} />
      </button>
    </div>
  );
}
