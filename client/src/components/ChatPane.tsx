import { createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { LogOut, Users, Gamepad2, MonitorX } from "lucide-solid";

import { useStore } from "../App";
import { MessageList } from "./MessageList";
import { Modal } from "./Modal";
import type {
  LaunchConfig,
  LaunchMode,
  MemberAssignment,
  PlayerRole,
} from "../types";
import "../styles/ChatPane.css";

export default function ChatPane() {
  const store = useStore();
  const [input, setInput] = createSignal("");
  const [membersOpen, setMembersOpen] = createSignal(false);

  // ── Launch modal state ──────────────────────────────────────────────────────
  const [showLaunchModal, setShowLaunchModal] = createSignal(false);
  const [selectedModeId, setSelectedModeId] = createSignal<string>("");
  const [gameList, setGameList] = createSignal<string[]>([]);
  const [selectedGame, setSelectedGame] = createSignal<string>("");
  const [assignments, setAssignments] = createSignal<
    Record<string, MemberAssignment>
  >({});

  const selectedMode = (): LaunchMode | undefined =>
    store.launchModes().find((m) => m.id === selectedModeId());

  const playerCount = () =>
    Object.values(assignments()).filter((a) => a.role === "Player").length;

  async function loadGameList(mode: LaunchMode | undefined): Promise<void> {
    if (!mode?.gamesFolder) {
      setGameList([]);
      return;
    }
    try {
      const games = await invoke<string[]>("list_games", {
        folder: mode.gamesFolder,
      });
      setGameList(games);
    } catch {
      setGameList([]);
    }
  }

  async function openLaunchModal(): Promise<void> {
    const a: Record<string, MemberAssignment> = {};
    store.members().forEach((m, i) => {
      a[m] =
        i < 8
          ? { role: "Player", player_id: i + 1 }
          : { role: "Spectator", player_id: 0 };
    });
    setAssignments(a);
    setSelectedGame("");

    const modes = store.launchModes();
    const firstMode = modes[0];
    setSelectedModeId(firstMode?.id ?? "");
    await loadGameList(firstMode);
    setShowLaunchModal(true);
  }

  async function handleSelectMode(id: string): Promise<void> {
    setSelectedModeId(id);
    setSelectedGame("");
    await loadGameList(store.launchModes().find((m) => m.id === id));
  }

  function setMemberRole(member: string, role: PlayerRole): void {
    setAssignments((prev) => {
      if (role === "Player") {
        const usedIds = new Set(
          Object.values(prev)
            .filter((a) => a.role === "Player")
            .map((a) => a.player_id),
        );
        let nextId = 1;
        while (usedIds.has(nextId)) nextId++;
        return { ...prev, [member]: { role, player_id: nextId } };
      }
      // Remove this member as a player, then compact remaining player IDs
      // so they stay contiguous starting from 1.
      const updated = { ...prev, [member]: { role, player_id: 0 } };
      const players = Object.entries(updated)
        .filter(([, a]) => a.role === "Player")
        .sort(([, a], [, b]) => a.player_id - b.player_id);
      players.forEach(([name], i) => {
        updated[name] = { ...updated[name], player_id: i + 1 };
      });
      return updated;
    });
  }

  function setMemberPlayerId(member: string, id: number): void {
    setAssignments((prev) => {
      // If another Player already holds this port, swap them.
      const clash = Object.keys(prev).find(
        (name) => name !== member && prev[name].role === "Player" && prev[name].player_id === id,
      );
      const myOldId = prev[member]?.player_id ?? 0;
      return {
        ...prev,
        [member]: { ...prev[member], player_id: id },
        ...(clash ? { [clash]: { ...prev[clash], player_id: myOldId } } : {}),
      };
    });
  }

  async function handleLaunch(): Promise<void> {
    const mode = selectedMode();
    if (!mode) return;
    if (mode.gamesFolder && !selectedGame()) return;
    const playerCount = Object.values(assignments()).filter(
      (a) => a.role === "Player",
    ).length;
    if (playerCount < 2) return;
    // Hash exe and (if applicable) game while modal is still open so members can verify integrity.
    const gameFilePath =
      mode.gamesFolder && selectedGame()
        ? `${mode.gamesFolder}/${selectedGame()}`
        : null;
    const [exeHash, gameHash] = await Promise.all([
      invoke<string>("hash_file", { path: mode.exePath }).catch(() => ""),
      gameFilePath
        ? invoke<string>("hash_file", { path: gameFilePath }).catch(() => "")
        : Promise.resolve(""),
    ]);
    const cfg: LaunchConfig = {
      game: selectedGame(),
      platform: mode.name,
      members: assignments(),
      ...(exeHash && { exe_hash: exeHash }),
      ...(gameHash && { game_hash: gameHash }),
    };
    setShowLaunchModal(false);
    await store.startGame(mode.exePath, cfg);
  }

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

  const sessionCtx = () =>
    store.activeChannel() === "session" ? store.currentSession() : null;

  return (
    <>
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
                  onClick={() =>
                    store.setShowMemberList(!store.showMemberList())
                  }
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
                          const isHost = () => m === store.currentHost();
                          const connState = () =>
                            isSelf()
                              ? null
                              : (store.peerStates().get(m) ?? null);
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
                              {isHost() ? " ★" : ""}
                              {isSelf() ? " (you)" : ""}
                            </div>
                          );
                        }}
                      </For>
                    </div>
                  </Show>
                </div>
                <Show when={store.gameStarted()}>
                  <span class="session-badge badge-in-game">In Game</span>
                </Show>
                <Show when={store.isHost()}>
                  <Show
                    when={!store.gameStarted()}
                    fallback={
                      <button
                        class="btn btn-danger"
                        onClick={() => store.stopGame()}
                      >
                        <MonitorX size={16} stroke-width={1.75} />
                      </button>
                    }
                  >
                    <button class="btn btn-primary" onClick={openLaunchModal}>
                      <Gamepad2 size={16} stroke-width={1.75} />
                    </button>
                  </Show>
                </Show>
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

      <Show when={showLaunchModal()}>
        <Modal title="Launch Game" onClose={() => setShowLaunchModal(false)} class="modal-lg">
          {/* Launch mode picker */}
          <div class="launch-section-label">Launch Mode</div>
          <Show
            when={store.launchModes().length > 0}
            fallback={
              <p class="launch-hint">
                No launch modes configured. Add one in Setup.
              </p>
            }
          >
            <select
              class="launch-select-full"
              value={selectedModeId()}
              onChange={(e) => handleSelectMode(e.currentTarget.value)}
            >
              <For each={store.launchModes()}>
                {(m) => <option value={m.id}>{m.name}</option>}
              </For>
            </select>
          </Show>

          {/* Game picker — only shown when the mode has a games folder */}
          <Show when={selectedMode()?.gamesFolder}>
            <div class="launch-section-label">Game</div>
            <Show
              when={gameList().length > 0}
              fallback={
                <p class="launch-hint">
                  No games found in the configured folder.
                </p>
              }
            >
              <div class="game-list-box">
                <For each={gameList()}>
                  {(g) => (
                    <div
                      class={`game-list-item${selectedGame() === g ? " selected" : ""}`}
                      onClick={() => setSelectedGame(g)}
                    >
                      {g}
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </Show>

          {/* Player assignments */}
          <div class="launch-section-label">Player Assignments</div>
          <For each={store.members()}>
            {(member) => {
              const a = () =>
                assignments()[member] ?? {
                  role: "Inactive" as PlayerRole,
                  player_id: 0,
                };
              return (
                <div class="launch-row">
                  <span class="launch-row-label">{member}</span>
                  <select
                    class="launch-role-select"
                    value={a().role}
                    onChange={(e) =>
                      setMemberRole(member, e.currentTarget.value as PlayerRole)
                    }
                  >
                    <option value="Player">Player</option>
                    <option value="Spectator">Spectator</option>
                    <option value="Inactive">Inactive</option>
                  </select>
                  <Show when={a().role === "Player"}>
                    <select
                      class="launch-id-select"
                      value={a().player_id}
                      onChange={(e) =>
                        setMemberPlayerId(member, Number(e.currentTarget.value))
                      }
                    >
                      <For
                        each={Array.from(
                          { length: playerCount() },
                          (_, i) => i + 1,
                        )}
                      >
                        {(n) => <option value={n}>Port {n}</option>}
                      </For>
                    </select>
                  </Show>
                </div>
              );
            }}
          </For>

          <div class="modal-actions">
            <button
              class="btn btn-secondary"
              onClick={() => setShowLaunchModal(false)}
            >
              Cancel
            </button>
            <button
              class="btn btn-primary"
              onClick={handleLaunch}
              disabled={
                !selectedModeId() ||
                (!!selectedMode()?.gamesFolder && !selectedGame()) ||
                Object.values(assignments()).filter((a) => a.role === "Player").length < 2
              }
            >
              Launch
            </button>
          </div>
        </Modal>
      </Show>
    </>
  );
}
