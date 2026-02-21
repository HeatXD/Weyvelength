import { createSignal } from "solid-js";
import { load, Store } from "@tauri-apps/plugin-store";

import type { ConnectionStatus, SavedServer, ServerInfo } from "../types";

const STORE_FILE = "servers.json";
const STORE_KEY = "servers";

export function createServerSlice() {
  const [servers, setServers] = createSignal<SavedServer[]>([]);
  const [activeServerId, setActiveServerId] = createSignal<string | null>(null);
  const [connectionStatus, setConnectionStatus] =
    createSignal<ConnectionStatus>("disconnected");
  const [serverInfo, setServerInfo] = createSignal<ServerInfo | null>(null);

  let store: Store | null = null;

  async function initServers(): Promise<void> {
    store = await load(STORE_FILE);
    const saved = await store.get<SavedServer[]>(STORE_KEY);
    if (saved) setServers(saved);
  }

  async function persistServers(servers: SavedServer[]): Promise<void> {
    await store!.set(STORE_KEY, servers);
    await store!.save();
  }

  function username(): string {
    const id = activeServerId();
    if (!id) return "";
    return servers().find((s) => s.id === id)?.username ?? "";
  }

  function addServer(data: Omit<SavedServer, "id">): string {
    const id = crypto.randomUUID();
    const updated = [...servers(), { id, ...data }];
    setServers(updated);
    void persistServers(updated);
    return id;
  }

  // Low-level mutation; public removeServer (which calls disconnect first) lives in index.ts.
  function removeFromList(id: string): void {
    const updated = servers().filter((s) => s.id !== id);
    setServers(updated);
    void persistServers(updated);
  }

  return {
    servers,
    setServers,
    activeServerId,
    setActiveServerId,
    connectionStatus,
    setConnectionStatus,
    serverInfo,
    setServerInfo,
    username,
    addServer,
    removeFromList,
    initServers,
  };
}
