import { createSignal } from "solid-js";
import { Minus, Plus } from "lucide-solid";

import { useStore } from "../App";
import { Modal } from "./Modal";

export default function CreateSessionModal() {
  const store = useStore();
  const [maxMembers, setMaxMembers] = createSignal(10);

  async function create(isPublic: boolean) {
    store.setShowCreateSession(false);
    await store.createSession(isPublic, maxMembers());
  }

  return (
    <Modal title="New Session" onClose={() => store.setShowCreateSession(false)}>
      <p style={{ color: "var(--text-muted)", "font-size": "0.875rem" }}>
        Choose session visibility. Private sessions require a code to join.
      </p>
      <div class="form-group">
        <label class="form-label">Max Members</label>
        <div class="size-stepper">
          <button
            class="btn btn-secondary stepper-btn"
            onClick={() => setMaxMembers((n) => Math.max(2, n - 1))}
          >
            <Minus size={16} stroke-width={2} />
          </button>
          <span class="stepper-value">{maxMembers()}</span>
          <button
            class="btn btn-secondary stepper-btn"
            onClick={() => setMaxMembers((n) => n + 1)}
          >
            <Plus size={16} stroke-width={2} />
          </button>
        </div>
      </div>
      <div class="modal-actions">
        <button
          class="btn btn-secondary"
          onClick={() => store.setShowCreateSession(false)}
        >
          Cancel
        </button>
        <button class="btn btn-private" onClick={() => create(false)}>
          Private
        </button>
        <button class="btn btn-primary" onClick={() => create(true)}>
          Public
        </button>
      </div>
    </Modal>
  );
}
