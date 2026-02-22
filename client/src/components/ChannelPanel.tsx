import { createSignal, For, Match, Show, Switch, createMemo } from "solid-js";
import { KeyRound, Plus, Users } from "lucide-solid";

import { useStore } from "../App";
import { FormField } from "./FormField";
import { Modal } from "./Modal";
import { Dropdown } from "./Dropdown";
import "../styles/ChannelPanel.css";

export default function ChannelPanel() {
  const store = useStore();
  const [joinCode, setJoinCode] = createSignal("");
  const [showJoinModal, setShowJoinModal] = createSignal(false);
  const hasTurnServers = createMemo(() => store.turnServers().length > 0);

  async function handleJoinByCode() {
    const code = joinCode().trim().toUpperCase();
    if (!code) return;
    setJoinCode("");
    setShowJoinModal(false);
    await store.enterSession(code);
  }

  return (
    <>
      <div class="channel-panel">
        <Switch>
          <Match when={store.connectionStatus() === "connected"}>
            <div class="server-header">
              <div class="server-name">
                {store.serverInfo()?.server_name ??
                  store.servers().find((s) => s.id === store.activeServerId())
                    ?.displayName ??
                  "Server"}
              </div>
              <div class="server-motd">{store.serverInfo()?.motd ?? ""}</div>
              <Show when={hasTurnServers()}>
                <br />
                <div class="turn-selector">
                  <span class="turn-selector-label">Relay</span>
                  <Dropdown
                    options={[
                      { label: "Direct only", value: null as string | null },
                      ...store.turnServers().map((s) => ({
                        label: s.name,
                        value: s.name as string | null,
                      })),
                    ]}
                    value={store.selectedTurn() as string | null}
                    onChange={(v) => store.setTurn(v)}
                  />
                </div>
              </Show>
            </div>

            <div class="channel-list">
              <div
                class={`channel-item ${store.activeChannel() === "global" ? "active" : ""}`}
                onClick={() => store.setActiveChannel("global")}
              >
                <span class="channel-item-name"># global</span>
                <span class="channel-item-count">
                  <Users size={12} stroke-width={1.75} />
                  {store.globalMembers().length}
                </span>
              </div>
              <Show when={store.currentSession()}>
                {(session) => {
                  const count = () =>
                    store.sessions().find((s) => s.id === session().session_id)
                      ?.member_count;
                  return (
                    <div
                      class={`channel-item ${store.activeChannel() === "session" ? "active" : ""}`}
                      onClick={() => store.setActiveChannel("session")}
                    >
                      <span class="channel-item-name">
                        # {session().session_name}
                      </span>
                      <Show when={session().is_public}>
                        <span class="channel-item-count">
                          <Users size={12} stroke-width={1.75} />
                          {count() ?? 0}/{session().max_members}
                        </span>
                      </Show>
                      <span
                        class={`session-badge ${session().is_public ? "badge-public" : "badge-private"}`}
                      >
                        {session().is_public ? "pub" : "prv"}
                      </span>
                    </div>
                  );
                }}
              </Show>
            </div>

            <div class="sessions-section">
              <div class="sessions-section-header">
                <span>Sessions</span>
                <div class="sessions-header-actions">
                  <button
                    class="btn btn-secondary sessions-icon-btn"
                    onClick={() => setShowJoinModal(true)}
                    title="Join by code"
                    disabled={!!store.currentSession()}
                  >
                    <KeyRound size={14} stroke-width={1.75} />
                  </button>
                  <button
                    class="btn btn-primary sessions-icon-btn"
                    onClick={() => store.setShowCreateSession(true)}
                    title="New session"
                    disabled={!!store.currentSession()}
                  >
                    <Plus size={14} stroke-width={2} />
                  </button>
                </div>
              </div>
              <For
                each={store.sessions()}
                fallback={<p class="channel-empty">No public sessions</p>}
              >
                {(session) => {
                  const isCurrent = () =>
                    store.currentSession()?.session_id === session.id;
                  const isFull = () =>
                    session.max_members > 0 &&
                    session.member_count >= session.max_members;
                  const isDisabled = () =>
                    !isCurrent() && (!!store.currentSession() || isFull());
                  return (
                    <div
                      class={`session-item${isCurrent() ? " session-item-current" : ""}${isDisabled() ? " session-item-disabled" : ""}`}
                      onClick={() =>
                        !isCurrent() &&
                        !isDisabled() &&
                        store.enterSession(session.id)
                      }
                    >
                      <span class="session-item-name">{session.name}</span>
                      <Show when={session.is_public}>
                        <span class="session-item-count">
                          <Users size={12} stroke-width={1.75} />
                          {session.member_count}/{session.max_members}
                        </span>
                      </Show>
                      <Show when={isFull() && !isCurrent()}>
                        <span class="session-badge badge-full">Full</span>
                      </Show>
                    </div>
                  );
                }}
              </For>
            </div>
          </Match>

          <Match when={store.connectionStatus() === "connecting"}>
            <div class="channel-panel-empty">
              <p>Connecting…</p>
            </div>
          </Match>

          <Match when={store.connectionStatus() === "disconnected"}>
            <div class="channel-panel-empty">
              <p>Add a server to get started</p>
            </div>
          </Match>
        </Switch>
      </div>

      <Show when={showJoinModal()}>
        <Modal title="Join by Code" onClose={() => setShowJoinModal(false)}>
          <FormField
            label="Lobby Code"
            placeholder="ABCD1234"
            value={joinCode()}
            onInput={(v) => setJoinCode(v.toUpperCase())}
          />
          <div class="modal-actions">
            <button
              class="btn btn-secondary"
              onClick={() => setShowJoinModal(false)}
            >
              Cancel
            </button>
            <button
              class="btn btn-primary"
              onClick={handleJoinByCode}
              disabled={!joinCode().trim()}
            >
              Join
            </button>
          </div>
        </Modal>
      </Show>
    </>
  );
}
