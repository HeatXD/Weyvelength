import { createSignal } from "solid-js";
import { load, Store } from "@tauri-apps/plugin-store";

import type { LaunchMode, UserConfig } from "../types";

// Reuse the same store file as the server slice; tauri-plugin-store deduplicates by path.
const STORE_FILE = "servers.json";
const CONFIG_KEY = "user-config";

export function createConfigSlice() {
  const [launchModes, setLaunchModes] = createSignal<LaunchMode[]>([]);
  const [showLaunchModeModal, setShowLaunchModeModal] = createSignal(false);
  const [debugLog, setDebugLog] = createSignal(false);

  let store: Store | null = null;

  async function initConfig(): Promise<void> {
    store = await load(STORE_FILE);
    const saved = await store.get<UserConfig>(CONFIG_KEY);
    if (saved?.launchModes) setLaunchModes(saved.launchModes);
    if (saved?.debugLog) setDebugLog(saved.debugLog);
  }

  async function persistConfig(): Promise<void> {
    const cfg: UserConfig = { launchModes: launchModes(), debugLog: debugLog() };
    await store!.set(CONFIG_KEY, cfg);
    await store!.save();
  }

  function addLaunchMode(data: Omit<LaunchMode, "id">): void {
    const updated = [...launchModes(), { id: crypto.randomUUID(), ...data }];
    setLaunchModes(updated);
    void persistConfig();
  }

  function removeLaunchMode(id: string): void {
    const updated = launchModes().filter((m) => m.id !== id);
    setLaunchModes(updated);
    void persistConfig();
  }

  function updateLaunchMode(id: string, data: Omit<LaunchMode, "id">): void {
    const updated = launchModes().map((m) =>
      m.id === id ? { id, ...data } : m,
    );
    setLaunchModes(updated);
    void persistConfig();
  }

  function toggleDebugLog(): void {
    setDebugLog((v) => !v);
    void persistConfig();
  }

  return {
    launchModes,
    showLaunchModeModal,
    setShowLaunchModeModal,
    debugLog,
    toggleDebugLog,
    initConfig,
    addLaunchMode,
    removeLaunchMode,
    updateLaunchMode,
  };
}
