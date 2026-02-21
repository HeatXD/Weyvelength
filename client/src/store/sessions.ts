import { createSignal } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import type { SessionInfo, SessionPayload } from "../types";

export function createSessionSlice(setError: (msg: string) => void) {
  const [sessions, setSessions] = createSignal<SessionInfo[]>([]);
  const [currentSession, setCurrentSession] =
    createSignal<SessionPayload | null>(null);
  const [members, setMembers] = createSignal<string[]>([]);
  const [globalMembers, setGlobalMembers] = createSignal<string[]>([]);

  async function refreshSessions(): Promise<void> {
    try {
      const list = await invoke<SessionInfo[]>("list_sessions");
      setSessions(list);
    } catch (e) {
      setError(String(e));
    }
  }

  async function fetchMembers(sessionId: string): Promise<void> {
    try {
      const list = await invoke<string[]>("get_members", { sessionId });
      setMembers(list);
    } catch (e) {
      setError(String(e));
    }
  }

  return {
    sessions,
    setSessions,
    currentSession,
    setCurrentSession,
    members,
    setMembers,
    globalMembers,
    setGlobalMembers,
    refreshSessions,
    fetchMembers,
  };
}
