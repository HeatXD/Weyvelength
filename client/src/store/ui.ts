import { createSignal } from "solid-js";

export function createUiSlice() {
  const [error, setErrorSignal] = createSignal("");
  const [showAddServer, setShowAddServer] = createSignal(false);
  const [showCreateSession, setShowCreateSession] = createSignal(false);
  const [showMemberList, setShowMemberList] = createSignal(false);

  let errorTimer: ReturnType<typeof setTimeout> | undefined;

  function setError(raw: string) {
    clearTimeout(errorTimer);
    const match = raw.match(/message: "([^"]+)"/);
    const msg = match ? match[1] : raw;
    setErrorSignal(msg);
    if (msg) {
      errorTimer = setTimeout(() => setErrorSignal(""), 4000);
    }
  }

  return {
    error,
    setError,
    showAddServer,
    setShowAddServer,
    showCreateSession,
    setShowCreateSession,
    showMemberList,
    setShowMemberList,
  };
}
