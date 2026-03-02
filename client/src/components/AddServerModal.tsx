import { createSignal } from "solid-js";

import { useStore } from "../App";
import { FormField } from "./FormField";
import { Modal } from "./Modal";

export default function AddServerModal() {
  const store = useStore();
  const [displayName, setDisplayName] = createSignal("");
  const [host, setHost] = createSignal("127.0.0.1");
  const [port, setPort] = createSignal("50051");

  function handleSubmit() {
    const p = parseInt(port());
    if (!displayName().trim() || isNaN(p)) return;
    const newId = store.addServer({
      displayName: displayName().trim(),
      host: host().trim(),
      port: p,
    });
    store.setShowAddServer(false);
    store.connectToServer(newId);
  }

  return (
    <Modal title="Add Server" onClose={() => store.setShowAddServer(false)}>
      <FormField
        label="Server Name"
        placeholder="My Server"
        value={displayName()}
        onInput={setDisplayName}
      />
      <div class="modal-row">
        <FormField
          label="Host"
          placeholder="127.0.0.1"
          value={host()}
          onInput={setHost}
          style={{ flex: "3" }}
        />
        <FormField
          label="Port"
          placeholder="50051"
          value={port()}
          onInput={setPort}
          style={{ flex: "1" }}
        />
      </div>
      <div class="modal-actions">
        <button
          class="btn btn-secondary"
          onClick={() => store.setShowAddServer(false)}
        >
          Cancel
        </button>
        <button
          class="btn btn-primary"
          onClick={handleSubmit}
          disabled={!displayName().trim()}
        >
          Add &amp; Connect
        </button>
      </div>
    </Modal>
  );
}
