import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { load, Store } from "@tauri-apps/plugin-store";

import type {
  ActiveChannel,
  ChatMessage,
  ConnectionStatus,
  LaunchConfig,
  LaunchMode,
  IceServer,
  SavedServer,
  ServerInfo,
  SessionInfo,
  SessionPayload,
} from "../types";
import { manageStream, StreamHandle } from "./streams";
import { createConfigSlice } from "./config";
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

/** Stop a backend stream, re-register the Tauri event listener, then restart the stream. */
async function restartStream<T>(
  handle: StreamHandle,
  stopCommand: string,
  eventName: string,
  startCommand: string,
  callback: (payload: T) => void,
): Promise<void> {
  teardownHandle(handle);
  await invoke(stopCommand);
  handle.unlisten = await listen<T>(eventName, (e) => callback(e.payload));
  await invoke(startCommand);
}

// ── Per-server auth token persistence ────────────────────────────────────────

const TOKEN_TTL_MS = 7 * 24 * 60 * 60 * 1000;

interface SavedSession {
  username: string;
  token: string;
  expiresAt: number;
}

let authStore: Store | null = null;
async function getAuthStore(): Promise<Store> {
  if (!authStore) authStore = await load("auth.json");
  return authStore;
}

async function loadSavedSession(serverId: string): Promise<SavedSession | null> {
  try {
    return (await (await getAuthStore()).get<SavedSession>(serverId)) ?? null;
  } catch {
    return null;
  }
}

async function saveSession(serverId: string, username: string, token: string): Promise<void> {
  try {
    const s = await getAuthStore();
    await s.set(serverId, { username, token, expiresAt: Date.now() + TOKEN_TTL_MS });
    await s.save();
  } catch { /* not critical */ }
}

async function clearSession(serverId: string): Promise<void> {
  try {
    const s = await getAuthStore();
    await s.delete(serverId);
    await s.save();
  } catch { /* not critical */ }
}

export interface AppStore {
  /** True while the auth modal (login / register) should be shown. */
  showAuthModal: () => boolean;
  /** Called by AuthModal on successful login/register. Receives the username and issued token. */
  onAuthSuccess: (username: string, token: string) => void;
  /** Called by AuthModal when the user dismisses without authenticating. */
  onAuthCancel: () => void;
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
  /** True when the current user is the session host. */
  isHost: () => boolean;
  /** True when a game is currently running in the session. */
  gameStarted: () => boolean;
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
  /** Configured launch mode profiles (name, exe path, games folder). */
  launchModes: () => LaunchMode[];
  addLaunchMode: (data: Omit<LaunchMode, "id">) => void;
  removeLaunchMode: (id: string) => void;
  /** True when the launch mode setup modal should be shown. */
  showLaunchModeModal: () => boolean;
  setShowLaunchModeModal: (v: boolean) => void;
  username: () => string;
  addServer: (s: Omit<SavedServer, "id">) => string;
  removeServer: (id: string) => void;
  connectToServer: (id: string) => Promise<void>;
  disconnect: () => Promise<void>;
  /** Clears the stored auth token for the current server then disconnects. */
  logout: () => Promise<void>;
  refreshSessions: () => Promise<void>;
  enterSession: (sessionId: string) => Promise<void>;
  createSession: (isPublic: boolean, maxMembers: number) => Promise<void>;
  leaveSession: () => Promise<void>;
  sendMessage: (content: string) => Promise<void>;
  fetchMembers: (sessionId: string) => Promise<void>;
  /** Host-only: send StartGame signal then launch the local emulator. */
  startGame: (exePath: string, cfg: LaunchConfig) => Promise<void>;
  /** Host-only: send StopGame signal (Tauri backend kills all processes on signal receipt). */
  stopGame: () => Promise<void>;
}

export function createAppStore(): AppStore {
  const ui = createUiSlice();
  const server = createServerSlice();
  void server.initServers();
  const config = createConfigSlice();
  void config.initConfig();
  const session = createSessionSlice(ui.setError);
  const messaging = createMessagingSlice();
  const [showAuthModal, setShowAuthModal] = createSignal(false);
  const [loggedInUsername, setLoggedInUsername] = createSignal("");
  let pendingAuthResolve: ((ok: boolean) => void) | null = null;

  function onAuthSuccess(username: string, token: string): void {
    setLoggedInUsername(username);
    setShowAuthModal(false);
    pendingAuthResolve?.(true);
    pendingAuthResolve = null;
    const id = server.activeServerId();
    if (id) void saveSession(id, username, token);
  }

  function onAuthCancel(): void {
    setShowAuthModal(false);
    pendingAuthResolve?.(false);
    pendingAuthResolve = null;
  }

  const [selectedTurn, setSelectedTurnSignal] = createSignal<string | null>(
    null,
  );
  const [forceRelay, setForceRelay] = createSignal<boolean>(false);
  const [currentHost, setCurrentHost] = createSignal<string>("");
  const [isHost, setIsHost] = createSignal<boolean>(false);
  const [gameStarted, setGameStarted] = createSignal<boolean>(false);
  const [peerStates, setPeerStates] = createSignal<Map<string, string>>(
    new Map(),
  );

  // Presence + update stream handles (touch index.ts state only, no dedicated slice needed)
  const sessionUpdatesHandle: StreamHandle = { unlisten: null };
  const globalMembersHandle: StreamHandle = { unlisten: null };
  const peerStateHandle: StreamHandle = { unlisten: null };
  const memberEventHandle: StreamHandle = { unlisten: null };
  const hostChangedHandle: StreamHandle = { unlisten: null };
  const gameStartedHandle: StreamHandle = { unlisten: null };
  const gameStoppedHandle: StreamHandle = { unlisten: null };
  // Kept alive for the entire app session; cleaned up in disconnect().
  const connectionLostHandle: StreamHandle = { unlisten: null };

  let joiningSession = false;
  let launchingGame = false;
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
    if (server.activeServerId() === id) return;
    if (server.connectionStatus() !== "disconnected") await disconnect();
    const srv = server.servers().find((s) => s.id === id);
    if (!srv) return;
    ui.setError("");
    server.setConnectionStatus("connecting");
    try {
      await invoke("connect", { host: srv.host, port: srv.port });
      // Show server as selected immediately so the icon highlights right away.
      server.setActiveServerId(id);

      // Try to restore a saved token before falling back to the auth modal.
      const saved = await loadSavedSession(id);
      let authed = false;
      if (saved && saved.expiresAt > Date.now()) {
        await invoke("restore_session", { username: saved.username, token: saved.token });
        setLoggedInUsername(saved.username);
        authed = true;
      } else {
        authed = await new Promise<boolean>((resolve) => {
          pendingAuthResolve = resolve;
          setShowAuthModal(true);
        });
      }

      if (!authed) {
        await invoke("disconnect").catch(() => {});
        server.setActiveServerId(null);
        server.setConnectionStatus("disconnected");
        return;
      }

      const info = await invoke<ServerInfo>("get_server_info");
      server.setServerInfo(info);
      server.setConnectionStatus("connected");
      session.setCurrentSession(null);
      messaging.setActiveChannel("global");

      // Prompt setup if no launch modes have been configured yet.
      if (config.launchModes().length === 0) {
        config.setShowLaunchModeModal(true);
      }

      // One-time listener: detect unexpected server disconnect.
      if (!connectionLostHandle.unlisten) {
        connectionLostHandle.unlisten = await listen("connection-lost", () => {
          if (server.connectionStatus() !== "disconnected") {
            disconnect().catch(() => {});
          }
        });
      }

      // Session-list push stream.
      await restartStream<SessionInfo[]>(
        sessionUpdatesHandle,
        "stop_session_updates_stream",
        "session-update",
        "start_session_updates_stream",
        (p) => session.setSessions(p),
      );

      // Global presence (online member list) stream.
      await restartStream<string[]>(
        globalMembersHandle,
        "stop_global_members_stream",
        "global-members",
        "start_global_members_stream",
        (p) => session.setGlobalMembers(p),
      );

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
      server.setActiveServerId(null);
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
      } catch {
        // WebRTC resources may already be released; not an error during disconnect.
      }
    }
    for (const h of [
      messaging.globalHandle,
      messaging.sessionHandle,
      peerStateHandle,
      memberEventHandle,
      hostChangedHandle,
      gameStartedHandle,
      gameStoppedHandle,
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
    setIsHost(false);
    setGameStarted(false);
    session.setSessions([]);
    session.setCurrentSession(null);
    messaging.setGlobalMessages([]);
    messaging.setSessionMessages([]);
    session.setMembers([]);
    session.setGlobalMembers([]);
    setLoggedInUsername("");
  }

  async function logout(): Promise<void> {
    const id = server.activeServerId();
    if (id) await clearSession(id);
    await disconnect();
  }

  async function joinSessionChannel(sess: SessionPayload): Promise<void> {
    setPeerStates(new Map()); // reset on each new session join
    setCurrentHost(sess.host);
    setIsHost(sess.host === loggedInUsername());
    setGameStarted(sess.game_started ?? false);
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
        } else if (
          state === "failed" &&
          !failedPeers.has(peer) &&
          !leftPeers.has(peer) &&
          session.currentSession()
        ) {
          failedPeers.add(peer);
          // Only auto-leave if we have no other open peer connections — i.e. this
          // failure leaves us completely isolated. If we're still connected to
          // other peers, just show an error and stay in the session.
          const hasOtherOpen = [...peerStates().entries()].some(
            ([p, s]) => p !== peer && s === "open",
          );
          if (!isHost() && !hasOtherOpen) {
            leaveSession()
              .then(() => {
                ui.setError(`Could not connect to ${peer}. Leaving session.`);
              })
              .catch(() => {});
          } else {
            ui.setError(`Could not connect to ${peer}.`);
          }
        }
      },
    );

    // member-event: server-pushed join/leave signals — works for public and
    // private sessions alike.  Refresh the member list and inject a system message.
    teardownHandle(memberEventHandle);
    const self = loggedInUsername();

    function handleMemberEvent(payload: {
      username: string;
      joined: boolean;
    }): void {
      const { username, joined } = payload;
      if (username === self) return;

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
        // Peer left — close our side of the WebRTC connection.
        invoke("close_peer_connection", { peer: username }).catch(() => {});
        if (connectingPeers.has(username)) {
          // Never finished connecting; replace the "connecting…" message in-place.
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
      session.fetchMembers(sess.session_id).catch(() => {});
    }

    memberEventHandle.unlisten = await listen<{
      username: string;
      joined: boolean;
    }>("member-event", (e) => handleMemberEvent(e.payload));

    // host-changed: server signals host migration
    teardownHandle(hostChangedHandle);
    hostChangedHandle.unlisten = await listen<string>("host-changed", (e) => {
      const newHost = e.payload;
      setCurrentHost(newHost);
      setIsHost(newHost === loggedInUsername());
      messaging.setSessionMessages((prev) => [
        ...prev,
        {
          username: "",
          content:
            newHost === loggedInUsername()
              ? "You are now the host"
              : `${newHost} is now the host`,
          timestamp: Date.now(),
          system: true,
        },
      ]);
    });

    // game-started: server fanned out GAME_STARTED signal to all session members.
    // Non-hosts get a file picker to choose their local emulator executable.
    teardownHandle(gameStartedHandle);
    gameStartedHandle.unlisten = await listen<string>("game-started", (e) => {
      console.log("[game-started] received, isHost=", isHost(), "payload=", e.payload);
      setGameStarted(true);
      sysMsg("The host started the game");
      if (!isHost()) {
        const cfg = JSON.parse(e.payload) as LaunchConfig;
        const me = cfg.members[loggedInUsername()];
        console.log("[game-started] non-host launch: me=", me, "username=", loggedInUsername());
        if (me?.role !== "Inactive") {
          void handleNonHostGameStart(me?.player_id ?? 0, cfg);
        }
      }
    });

    // game-stopped: reset UI state. If we're the host and the process exited
    // naturally (watcher fired this), also tell the server so it fans GAME_STOPPED
    // out to non-hosts — otherwise they'd stay stuck on "In Game".
    teardownHandle(gameStoppedHandle);
    gameStoppedHandle.unlisten = await listen("game-stopped", () => {
      const wasHost = isHost();
      const wasRunning = gameStarted();
      setGameStarted(false);
      if (wasRunning) sysMsg("The game has ended");
      if (wasHost && wasRunning) {
        void invoke("stop_game").catch(() => {});
      }
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
    await withJoinGuard(() =>
      invoke<SessionPayload>("create_session", { isPublic, maxMembers }),
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
    teardownHandle(gameStartedHandle);
    teardownHandle(gameStoppedHandle);
    setPeerStates(new Map());
    setCurrentHost("");
    setIsHost(false);
    setGameStarted(false);
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

  function sysMsg(content: string): void {
    messaging.setSessionMessages((prev) => [
      ...prev,
      { username: "", content, timestamp: Date.now(), system: true },
    ]);
  }

  async function handleNonHostGameStart(
    playerId: number,
    cfg: LaunchConfig,
  ): Promise<void> {
    const modes = config.launchModes();
    console.log("[handleNonHostGameStart] platform=", cfg.platform, "modes=", modes.map((m) => m.name));
    const match =
      modes.find((m) => m.name.toLowerCase() === cfg.platform.toLowerCase()) ??
      (modes.length === 1 ? modes[0] : undefined);
    if (!match) {
      console.warn("[handleNonHostGameStart] no matching launch mode");
      sysMsg(
        modes.length === 0
          ? `Game launch failed: no launch modes configured — open Weyvelength Setup to add one.`
          : `Game launch failed: no launch mode named "${cfg.platform}" — open Weyvelength Setup to configure it.`,
      );
      return;
    }
    console.log("[handleNonHostGameStart] matched mode=", match.name, "exePath=", match.exePath);

    // Hash check — only verify fields the host included.
    if (cfg.exe_hash || cfg.game_hash) {
      const gamePath =
        match.gamesFolder && cfg.game
          ? `${match.gamesFolder}/${cfg.game}`
          : null;
      const [exeHash, gameHash] = await Promise.all([
        cfg.exe_hash
          ? invoke<string>("hash_file", { path: match.exePath }).catch(
              () => null,
            )
          : Promise.resolve<string | null>(null),
        cfg.game_hash && gamePath
          ? invoke<string>("hash_file", { path: gamePath }).catch(() => null)
          : Promise.resolve<string | null>(null),
      ]);

      const mismatches: string[] = [];
      if (cfg.exe_hash) {
        if (exeHash === null)
          mismatches.push(`executable not found at "${match.exePath}"`);
        else if (exeHash !== cfg.exe_hash)
          mismatches.push(`executable is a different version from the host's`);
      }
      if (cfg.game_hash) {
        if (!gamePath)
          mismatches.push(`no games folder configured for "${cfg.platform}"`);
        else if (gameHash === null)
          mismatches.push(`"${cfg.game}" not found in ${match.gamesFolder}`);
        else if (gameHash !== cfg.game_hash)
          mismatches.push(
            `"${cfg.game}" is a different version from the host's`,
          );
      }

      if (mismatches.length > 0) {
        console.warn("[handleNonHostGameStart] hash mismatch:", mismatches);
        sysMsg(`Launch cancelled — hash mismatch: ${mismatches.join("; ")}.`);
        return;
      }
    }

    console.log("[handleNonHostGameStart] invoking launch_game playerId=", playerId);
    const { exe_hash: _e, game_hash: _g, ...launchCfg } = cfg;
    await invoke("launch_game", {
      exePath: match.exePath,
      playerId,
      config: JSON.stringify(launchCfg),
    }).catch((e: unknown) => {
      sysMsg(`Game launch failed: ${String(e)}`);
    });
  }

  async function startGame(exePath: string, cfg: LaunchConfig): Promise<void> {
    const sess = session.currentSession();
    if (!sess || !isHost() || launchingGame) return;
    launchingGame = true;
    ui.setError("");
    const payload = JSON.stringify(cfg);
    try {
      await invoke("start_game", { sessionId: sess.session_id, payload });
      setGameStarted(true);
      const me = cfg.members[loggedInUsername()];
      if (me?.role !== "Inactive") {
        // Strip verification-only fields before handing config to the executable.
        const { exe_hash: _e, game_hash: _g, ...launchCfg } = cfg;
        await invoke("launch_game", {
          exePath,
          playerId: me?.player_id ?? 0,
          config: JSON.stringify(launchCfg),
        }).catch((e: unknown) => {
          sysMsg(`Game launch failed: ${String(e)}`);
        });
      }
    } catch (e) {
      ui.setError(String(e));
    } finally {
      launchingGame = false;
    }
  }

  async function stopGame(): Promise<void> {
    if (!isHost() || !gameStarted()) return;
    ui.setError("");
    try {
      await invoke("stop_game");
      // Host also receives GAME_STOPPED signal back → setGameStarted(false)
      // will fire in the listener; set it here for immediate UI response.
      setGameStarted(false);
    } catch (e) {
      ui.setError(String(e));
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
    // auth
    showAuthModal,
    onAuthSuccess,
    onAuthCancel,
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
    isHost,
    gameStarted,
    peerStates,
    username: loggedInUsername,
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
    // config slice
    launchModes: config.launchModes,
    addLaunchMode: config.addLaunchMode,
    removeLaunchMode: config.removeLaunchMode,
    showLaunchModeModal: config.showLaunchModeModal,
    setShowLaunchModeModal: config.setShowLaunchModeModal,
    // cross-cutting
    connectToServer,
    disconnect,
    logout,
    enterSession,
    createSession,
    leaveSession,
    sendMessage,
    startGame,
    stopGame,
  };
}
