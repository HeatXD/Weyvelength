import { createContext, onCleanup, Show, useContext } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import AddServerModal from "./components/AddServerModal";
import ChannelPanel from "./components/ChannelPanel";
import ChatPane from "./components/ChatPane";
import CreateSessionModal from "./components/CreateSessionModal";
import MemberListPanel from "./components/MemberListPanel";
import ServerSidebar from "./components/ServerSidebar";
import Toast from "./components/Toast";
import { AppStore, createAppStore } from "./store";
import "./styles/App.css";

export const StoreContext = createContext<AppStore>({} as AppStore);
export function useStore() {
  return useContext(StoreContext);
}

function App() {
  const store = createAppStore();

  // F5 / hard refresh: beforeunload fires but async can't be awaited.
  // Fire the IPC message synchronously so the backend receives it before the
  // webview reloads. onCleanup handles normal SolidJS teardown.
  function handleBeforeUnload() {
    invoke("disconnect").catch(() => {});
  }
  window.addEventListener("beforeunload", handleBeforeUnload);
  onCleanup(() => {
    window.removeEventListener("beforeunload", handleBeforeUnload);
    store.disconnect().catch(() => {});
  });

  return (
    <StoreContext.Provider value={store}>
      <div class="app-layout">
        <ServerSidebar />
        <ChannelPanel />
        <ChatPane />
        <MemberListPanel />
      </div>
      <Show when={store.showAddServer()}>
        <AddServerModal />
      </Show>
      <Show when={store.showCreateSession()}>
        <CreateSessionModal />
      </Show>
      <Toast />
    </StoreContext.Provider>
  );
}

export default App;
