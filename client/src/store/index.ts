import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  ActiveChannel,
  ChatMessage,
  ConnectionStatus,
  IceServer,
  SavedServer,
  ServerInfo,
  SessionInfo,
  SessionPayload,
} from "../types";
import { manageStream, StreamHandle } from "./streams";
import { createUiSlice } from "./ui";
import { createServerSlice } from "./server";
import { createSessionSlice } from "./sessions";
import { createMessagingSlice } from "./messaging";

function teardownHandle(h: StreamHandle): void {
  if (h.unlisten) {
    h.unlisten();
    h.unlisten = null;
  }
}

export interface AppStore {
  servers: () => SavedServer[];
  activeServerId: () => string | null;
  connectionStatus: () => ConnectionStatus;
  serverInfo: () => ServerInfo | null;
  /** TURN servers advertised by the connected server (name is non-empty). */
  turnServers: () => IceServer[];
  /** Name of the currently selected TURN server, or null for direct-only. */
  selectedTurn: () => string | null;
  setTurn: (name: string | null) => Promise<void>;
  /** When true, WebRTC will only use TURN relay candidates (forces relay path). */
  forceRelay: () => boolean;
  setForceRelay: (v: boolean) => void;
  /** Username of the current session host, or empty string when not in a session. */
  currentHost: () => string;
  /** WebRTC connection state per peer username. Values: "checking" | "connected" | "open" | "disconnected" | "failed" | "closed" */
  peerStates: () => Map<string, string>;
  sessions: () => SessionInfo[];
  currentSession: () => SessionPayload | null;
  activeChannel: () => ActiveChannel;
  setActiveChannel: (c: ActiveChannel) => void;
  globalMessages: () => ChatMessage[];
  sessionMessages: () => ChatMessage[];
  members: () => string[];
  globalMembers: () => string[];
  error: () => string;
  showAddServer: () => boolean;
  setShowAddServer: (v: boolean) => void;
  showCreateSession: () => boolean;
  setShowCreateSession: (v: boolean) => void;
  showMemberList: () => boolean;
  setShowMemberList: (v: boolean) => void;
  username: () => string;
  addServer: (s: Omit<SavedServer, "id">) => string;
  removeServer: (id: string) => void;
  connectToServer: (id: string) => Promise<void>;
  disconnect: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  enterSession: (sessionId: string) => Promise<void>;
  createSession: (isPublic: boolean, maxMembers: number) => Promise<void>;
  leaveSession: () => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  fetchMembers: (sessionId: string) => Promise<void>;
}

export function createAppStore(): AppStore {
  const ui = createUiSlice();
  const server = createServerSlice();
  void server.initServers();
  const session = createSessionSlice(ui.setError);
  const messaging = createMessagingSlice();
  const [selectedTurn, setSelectedTurnSignal] = createSignal<string | null>(
    null,
  );
  const [forceRelay, setForceRelay] = createSignal<boolean>(false);
  const [currentHost, setCurrentHost] = createSignal<string>("");
  const [peerStates, setPeerStates] = createSignal<Map<string, string>>(
    new Map(),
  );

  // Presence + update stream handles (touch index.ts state only, no dedicated slice needed)
  const sessionUpdatesHandle: StreamHandle = { unlisten: null };
  const globalMembersHandle: StreamHandle = { unlisten: null };
  const peerStateHandle: StreamHandle = { unlisten: null };
  const memberEventHandle: StreamHandle = { unlisten: null };
  const hostChangedHandle: StreamHandle = { unlisten: null };
  // Kept alive for the entire app session; cleaned up in disconnect().
  const connectionLostHandle: StreamHandle = { unlisten: null };

  let joiningSession = false;
  async function withJoinGuard(
    fn: () => Promise<SessionPayload>,
  ): Promise<void> {
    if (session.currentSession() || joiningSession) return;
    joiningSession = true;
    ui.setError("");
    try {
      await joinSessionChannel(await fn());
    } catch (e) {
      ui.setError(String(e));
    } finally {
      joiningSession = false;
    }
  }

  // ── Cross-cutting actions ──────────────────────────────────────────────────

  async function connectToServer(id: string): Promise<void> {
    const srv = server.servers().find((s) => s.id === id);
    if (!srv) return;
    ui.setError("");
    server.setConnectionStatus("connecting");
    try {
      await invoke("connect", {
        host: srv.host,
        port: srv.port,
        username: srv.username,
      });
      const info = await invoke<ServerInfo>("get_server_info");
      server.setServerInfo(info);
      server.setActiveServerId(id);
      server.setConnectionStatus("connected");
      session.setCurrentSession(null);
      messaging.setActiveChannel("global");

      // One-time listener: detect unexpected server disconnect.
      if (!connectionLostHandle.unlisten) {
        connectionLostHandle.unlisten = await listen("connection-lost", () => {
          if (server.connectionStatus() !== "disconnected") {
            disconnect().catch(() => {});
          }
        });
      }

      // Session-list push stream.
      teardownHandle(sessionUpdatesHandle);
      await invoke("stop_session_updates_stream");
      sessionUpdatesHandle.unlisten = await listen<SessionInfo[]>(
        "session-update",
        (e) => session.setSessions(e.payload),
      );
      await invoke("start_session_updates_stream");

      // Global presence (online member list) stream.
      teardownHandle(globalMembersHandle);
      await invoke("stop_global_members_stream");
      globalMembersHandle.unlisten = await listen<string[]>(
        "global-members",
        (e) => session.setGlobalMembers(e.payload),
      );
      await invoke("start_global_members_stream");

      // Global chat stream.
      await manageStream(
        messaging.globalHandle,
        "stop_global_stream",
        "global-message",
        messaging.setGlobalMessages,
        "start_global_stream",
      );
    } catch (e) {
      ui.setError(String(e));
      server.setConnectionStatus("disconnected");
    }
  }

  function turnServers(): IceServer[] {
    return (server.serverInfo()?.ice_servers ?? []).filter(
      (s) => s.name !== "",
    );
  }

  async function setTurn(name: string | null): Promise<void> {
    await invoke("set_turn_server", { name });
    setSelectedTurnSignal(name);
  }

  async function disconnect(): Promise<void> {
    // Clean up WebRTC if in a session
    if (session.currentSession()) {
      try {
        await invoke("leave_session_webrtc");
      } catch {}
    }
    for (const h of [
      messaging.globalHandle,
      messaging.sessionHandle,
      peerStateHandle,
      memberEventHandle,
      hostChangedHandle,
      sessionUpdatesHandle,
      globalMembersHandle,
      connectionLostHandle,
    ]) {
      teardownHandle(h);
    }
    try {
      await invoke("disconnect"); // cancels all Rust-side streams + clears state
    } catch {
      /* server may already be gone */
    }
    server.setActiveServerId(null);
    server.setConnectionStatus("disconnected");
    server.setServerInfo(null);
    setSelectedTurnSignal(null);
    setPeerStates(new Map());
    setCurrentHost("");
    session.setSessions([]);
    session.setCurrentSession(null);
    messaging.setGlobalMessages([]);
    messaging.setSessionMessages([]);
    session.setMembers([]);
    session.setGlobalMembers([]);
  }

  async function joinSessionChannel(sess: SessionPayload): Promise<void> {
    setPeerStates(new Map()); // reset on each new session join
    setCurrentHost(sess.host);
    let isOwner = sess.host === server.username();
    session.setCurrentSession(sess);
    messaging.setActiveChannel("session");
    messaging.setSessionMessages([]);

    // Populate the member list immediately so the panel is ready before opening.
    await session.fetchMembers(sess.session_id);

    // Pre-populate "connecting…" messages for all existing peers synchronously,
    // before any WebRTC events can fire — avoids race between on_peer_connection_state_change
    // ("checking") and on_open ("open") which are separate async Rust callbacks.
    const connectingPeers = new Set<string>(sess.existing_peers);
    const now = Date.now();
    if (sess.existing_peers.length > 0) {
      messaging.setSessionMessages(
        sess.existing_peers.map((peer) => ({
          username: "",
          content: `${peer} is connecting…`,
          timestamp: now,
          system: true,
        })),
      );
    }

    await invoke("join_session_webrtc", {
      sessionId: sess.session_id,
      existingPeers: sess.existing_peers,
      forceRelay: forceRelay(),
    });

    // Start the gRPC session chat stream (replaces WebRTC chat DC).
    await invoke("stop_session_stream");
    await invoke("start_session_stream", { sessionId: sess.session_id });

    // session-message: incoming gRPC session chat messages
    teardownHandle(messaging.sessionHandle);
    messaging.sessionHandle.unlisten = await listen<ChatMessage>(
      "session-message",
      (e) => messaging.setSessionMessages((prev) => [...prev, e.payload]),
    );

    // peer-state: WebRTC connection status per peer
    teardownHandle(peerStateHandle);
    const failedPeers = new Set<string>();
    const leftPeers = new Set<string>();
    peerStateHandle.unlisten = await listen<{ peer: string; state: string }>(
      "peer-state",
      (e) => {
        const { peer, state } = e.payload;
        setPeerStates((prev) => {
          const next = new Map(prev);
          next.set(peer, state);
          return next;
        });

        if (state === "open" && connectingPeers.has(peer)) {
          connectingPeers.delete(peer);
          messaging.setSessionMessages((prev) => {
            const idx = prev.findLastIndex(
              (m) => m.system && m.content === `${peer} is connecting…`,
            );
            if (idx === -1) return prev;
            const next = [...prev];
            next[idx] = { ...next[idx], content: `${peer} joined` };
            return next;
          });
        } else if (state === "failed" && !failedPeers.has(peer) && !leftPeers.has(peer) && session.currentSession()) {
          failedPeers.add(peer);
          // Only auto-leave if we have no other open peer connections — i.e. this
          // failure leaves us completely isolated. If we're still connected to
          // other peers, just show an error and stay in the session.
          const hasOtherOpen = [...peerStates().entries()].some(
            ([p, s]) => p !== peer && s === "open",
          );
          if (!isOwner && !hasOtherOpen) {
            leaveSession().then(() => {
              ui.setError(`Could not connect to ${peer}. Leaving session.`);
            }).catch(() => {});
          } else {
            ui.setError(`Could not connect to ${peer}.`);
          }
        }
      },
    );

    // member-event: server-pushed join/leave signals — works for public and
    // private sessions alike.  Refresh the member list and inject a system message.
    teardownHandle(memberEventHandle);
    const self = server.username();
    memberEventHandle.unlisten = await listen<{
      username: string;
      joined: boolean;
    }>("member-event", (e) => {
      const { username, joined } = e.payload;
      if (username !== self) {
        if (joined) {
          connectingPeers.add(username);
          messaging.setSessionMessages((prev) => [
            ...prev,
            {
              username: "",
              content: `${username} is connecting…`,
              timestamp: Date.now(),
              system: true,
            },
          ]);
        } else {
          invoke("close_peer_connection", { peer: username }).catch(() => {});
          if (connectingPeers.has(username)) {
            connectingPeers.delete(username);
            messaging.setSessionMessages((prev) => {
              const idx = prev.findLastIndex(
                (m) => m.system && m.content === `${username} is connecting…`,
              );
              if (idx === -1)
                return [
                  ...prev,
                  {
                    username: "",
                    content: `${username} left`,
                    timestamp: Date.now(),
                    system: true,
                  },
                ];
              const next = [...prev];
              next[idx] = { ...next[idx], content: `${username} left` };
              return next;
            });
          } else {
            leftPeers.add(username);
            messaging.setSessionMessages((prev) => [
              ...prev,
              {
                username: "",
                content: `${username} left`,
                timestamp: Date.now(),
                system: true,
              },
            ]);
          }
        }
      }
      session.fetchMembers(sess.session_id).catch(() => {});
    });

    // host-changed: server signals host migration
    teardownHandle(hostChangedHandle);
    hostChangedHandle.unlisten = await listen<string>("host-changed", (e) => {
      const newHost = e.payload;
      setCurrentHost(newHost);
      isOwner = newHost === server.username();
      messaging.setSessionMessages((prev) => [
        ...prev,
        {
          username: "",
          content:
            newHost === server.username()
              ? "You are now the host"
              : `${newHost} is now the host`,
          timestamp: Date.now(),
          system: true,
        },
      ]);
    });
  }

  async function enterSession(sessionId: string): Promise<void> {
    await withJoinGuard(() =>
      invoke<SessionPayload>("join_session", { sessionId }),
    );
  }

  async function createSession(
    isPublic: boolean,
    maxMembers: number,
  ): Promise<void> {
    await withJoinGuard(
      () => invoke<SessionPayload>("create_session", { isPublic, maxMembers }),
    );
  }

  async function leaveSession(): Promise<void> {
    const sess = session.currentSession();
    if (!sess) return;
    ui.setError("");
    try {
      await invoke("leave_session", { sessionId: sess.session_id });
    } catch (e) {
      ui.setError(String(e));
    }
    // Clear currentSession and knownSessionMembers here, synchronously, before
    // any further awaits.  Tauri IPC events (session-update) are macrotasks;
    // clearing state in this microtask continuation means the session-update
    // handler will see currentSession === null and skip doMemberDiff, avoiding
    // a "session not found" error from fetching a deleted session's member list.
    session.setCurrentSession(null);
    // Always clean up WebRTC and session stream regardless of gRPC success/failure
    await Promise.all([
      invoke("leave_session_webrtc").catch(() => {}),
      invoke("stop_session_stream").catch(() => {}),
    ]);
    teardownHandle(messaging.sessionHandle);
    teardownHandle(peerStateHandle);
    teardownHandle(memberEventHandle);
    teardownHandle(hostChangedHandle);
    setPeerStates(new Map());
    setCurrentHost("");
    messaging.setSessionMessages([]);
    messaging.setActiveChannel("global");
    session.setMembers([]);
  }

  function removeServer(id: string): void {
    server.removeFromList(id);
    if (server.activeServerId() === id) {
      disconnect().catch(() => {});
    }
  }

  async function sendMessage(content: string): Promise<void> {
    try {
      if (messaging.activeChannel() === "global") {
        await invoke("send_global_message", { content });
      } else {
        await invoke("send_session_message", { content });
      }
    } catch (e) {
      ui.setError(String(e));
    }
  }

  return {
    // server slice
    servers: server.servers,
    activeServerId: server.activeServerId,
    connectionStatus: server.connectionStatus,
    serverInfo: server.serverInfo,
    turnServers,
    selectedTurn,
    setTurn,
    forceRelay,
    setForceRelay,
    currentHost,
    peerStates,
    username: server.username,
    addServer: server.addServer,
    removeServer,
    // session slice
    sessions: session.sessions,
    currentSession: session.currentSession,
    members: session.members,
    globalMembers: session.globalMembers,
    refreshSessions: session.refreshSessions,
    fetchMembers: session.fetchMembers,
    // messaging slice
    globalMessages: messaging.globalMessages,
    sessionMessages: messaging.sessionMessages,
    activeChannel: messaging.activeChannel,
    setActiveChannel: messaging.setActiveChannel,
    // ui slice
    error: ui.error,
    showAddServer: ui.showAddServer,
    setShowAddServer: ui.setShowAddServer,
    showCreateSession: ui.showCreateSession,
    setShowCreateSession: ui.setShowCreateSession,
    showMemberList: ui.showMemberList,
    setShowMemberList: ui.setShowMemberList,
    // cross-cutting
    connectToServer,
    disconnect,
    enterSession,
    createSession,
    leaveSession,
    sendMessage,
  };
}
