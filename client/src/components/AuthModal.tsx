import { createSignal, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import { useStore } from "../App";
import { FormField } from "./FormField";
import { Modal } from "./Modal";

export default function AuthModal() {
  const store = useStore();

  const [tab, setTab] = createSignal<"login" | "register">("login");
  const [username, setUsername] = createSignal("");
  const [password, setPassword] = createSignal("");
  const [confirm, setConfirm] = createSignal("");
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal(false);

  async function handleSubmit() {
    setError("");
    const u = username().trim();
    const p = password();
    if (!u || !p) return;

    if (tab() === "register" && p !== confirm()) {
      setError("Passwords do not match.");
      return;
    }

    setBusy(true);
    try {
      if (tab() === "register") {
        await invoke("register", { username: u, password: p });
      }
      const token = await invoke<string>("login", { username: u, password: p });
      store.onAuthSuccess(u, token);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title="Sign In" onClose={() => store.onAuthCancel()}>
      <div class="auth-tabs">
        <button
          class={`auth-tab${tab() === "login" ? " active" : ""}`}
          onClick={() => { setTab("login"); setError(""); }}
        >
          Login
        </button>
        <button
          class={`auth-tab${tab() === "register" ? " active" : ""}`}
          onClick={() => { setTab("register"); setError(""); }}
        >
          Register
        </button>
      </div>
      <FormField
        label="Username"
        placeholder="Username"
        value={username()}
        onInput={setUsername}
      />
      <div class="form-group">
        <label>Password</label>
        <input
          type="password"
          placeholder="Password"
          value={password()}
          onInput={(e) => setPassword(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && void handleSubmit()}
        />
      </div>
      <Show when={tab() === "register"}>
        <div class="form-group">
          <label>Confirm Password</label>
          <input
            type="password"
            placeholder="Repeat password"
            value={confirm()}
            onInput={(e) => setConfirm(e.currentTarget.value)}
            onKeyDown={(e) => e.key === "Enter" && void handleSubmit()}
          />
        </div>
      </Show>
      <Show when={error()}>
        <p class="auth-error">{error()}</p>
      </Show>
      <div class="modal-actions">
        <button class="btn btn-secondary" onClick={() => store.onAuthCancel()}>
          Cancel
        </button>
        <button
          class="btn btn-primary"
          onClick={() => void handleSubmit()}
          disabled={busy() || !username().trim() || !password()}
        >
          {tab() === "login" ? "Login" : "Register"}
        </button>
      </div>
    </Modal>
  );
}
