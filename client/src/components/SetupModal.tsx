import { createSignal, For, Show } from "solid-js";
import { invoke } from "@tauri-apps/api/core";

import { useStore } from "../App";
import { FormField } from "./FormField";
import { Modal } from "./Modal";
import type { LaunchMode } from "../types";
import "../styles/SetupModal.css";

export default function SetupModal() {
  const store = useStore();
  const [adding, setAdding] = createSignal(false);
  const [name, setName] = createSignal("");
  const [exePath, setExePath] = createSignal("");
  const [gamesFolder, setGamesFolder] = createSignal("");

  function deriveName(path: string): string {
    return path.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, "") ?? "";
  }

  async function browseExe() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const sel = await open({
      multiple: false,
      filters: [{ name: "Executable", extensions: ["exe"] }],
    });
    const p = typeof sel === "string" ? sel : null;
    if (!p) return;
    setExePath(p);
    if (!name()) setName(deriveName(p));
  }

  async function browseFolder() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const sel = await open({ directory: true, multiple: false });
    const p = typeof sel === "string" ? sel : null;
    if (p) setGamesFolder(p);
  }

  async function handleAdd() {
    const n = name().trim() || deriveName(exePath());
    if (!exePath() || !n) return;
    const exeHash = await invoke<string>("hash_file", { path: exePath() }).catch(() => undefined);
    const data: Omit<LaunchMode, "id"> = {
      name: n,
      exePath: exePath(),
      ...(gamesFolder() && { gamesFolder: gamesFolder() }),
      ...(exeHash && { exeHash }),
    };
    store.addLaunchMode(data);
    setAdding(false);
    setName("");
    setExePath("");
    setGamesFolder("");
  }

  function handleCancel() {
    setAdding(false);
    setName("");
    setExePath("");
    setGamesFolder("");
  }

  return (
    <Modal
      title="Weyvelength Setup"
      onClose={() => store.setShowLaunchModeModal(false)}
    >
      <p class="setup-description">
        Configure executables and their game folders. These are used when
        launching or joining a game session.
      </p>

      <div class="launch-mode-list">
        <For
          each={store.launchModes()}
          fallback={
            <p class="setup-empty">
              No launch modes configured yet.
            </p>
          }
        >
          {(mode) => (
            <div class="launch-mode-item">
              <div class="launch-mode-info">
                <span class="launch-mode-name">{mode.name}</span>
                <span class="launch-mode-path" title={mode.exePath}>
                  {mode.exePath}
                </span>
                <span class="launch-mode-folder" title={mode.gamesFolder}>
                  {mode.gamesFolder}
                </span>
              </div>
              <button
                class="btn btn-danger btn-icon-sm"
                onClick={() => store.removeLaunchMode(mode.id)}
                title="Remove"
              >
                ✕
              </button>
            </div>
          )}
        </For>
      </div>

      <Show
        when={adding()}
        fallback={
          <button class="btn btn-secondary" onClick={() => setAdding(true)}>
            + Add
          </button>
        }
      >
        <div class="add-mode-form">
          <FormField
            label="Name"
            value={name()}
            onInput={setName}
            placeholder="SNES9x"
          />
          <div class="setup-path-row">
            <span class="setup-path" title={exePath()}>
              {exePath() || "No executable selected"}
            </span>
            <button class="btn btn-secondary" onClick={browseExe}>
              Browse Exe…
            </button>
          </div>
          <div class="setup-path-row">
            <span class="setup-path" title={gamesFolder()}>
              {gamesFolder() || "No folder (optional)"}
            </span>
            <button class="btn btn-secondary" onClick={browseFolder}>
              Browse Folder…
            </button>
          </div>
          <div class="setup-form-actions">
            <button class="btn btn-secondary" onClick={handleCancel}>
              Cancel
            </button>
            <button
              class="btn btn-primary"
              onClick={handleAdd}
              disabled={!exePath()}
            >
              Add
            </button>
          </div>
        </div>
      </Show>

      <div class="modal-actions">
        <button
          class="btn btn-primary"
          onClick={() => store.setShowLaunchModeModal(false)}
        >
          Done
        </button>
      </div>
    </Modal>
  );
}
