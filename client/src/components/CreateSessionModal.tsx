import { createSignal } from "solid-js";
import { Minus, Plus } from "lucide-solid";

import { useStore } from "../App";
import { Modal } from "./Modal";

const MIN_PLAYERS = 2;
const MAX_PLAYERS = 16;

export default function CreateSessionModal() {
  const store = useStore();
  const [maxMembers, setMaxMembers] = createSignal(8);
  const [isPublic, setIsPublic] = createSignal(true);

  async function create() {
    store.setShowCreateSession(false);
    await store.createSession(isPublic(), maxMembers());
  }

  return (
    <Modal
      title="Create Session"
      onClose={() => store.setShowCreateSession(false)}
    >
      <div class="session-form">
        {/* ── Max Players ────────────────────────────────────────────── */}
        <label class="form-label">Max Players</label>
        <div class="form-control-col">
          <div class="size-stepper">
            <button
              class="btn btn-secondary stepper-btn"
              disabled={maxMembers() <= MIN_PLAYERS}
              onClick={() => setMaxMembers((n) => Math.max(MIN_PLAYERS, n - 1))}
              aria-label="Decrease"
            >
              <Minus size={16} stroke-width={2.5} />
            </button>
            <span class="stepper-value">{maxMembers()}</span>
            <button
              class="btn btn-secondary stepper-btn"
              disabled={maxMembers() >= MAX_PLAYERS}
              onClick={() => setMaxMembers((n) => Math.min(MAX_PLAYERS, n + 1))}
              aria-label="Increase"
            >
              <Plus size={16} stroke-width={2.5} />
            </button>
          </div>
          <p class="form-hint">
            {MIN_PLAYERS} – {MAX_PLAYERS} players
          </p>
        </div>

        {/* ── Visibility ─────────────────────────────────────────────── */}
        <label class="form-label">Visibility</label>
        <div class="form-control-col">
          <div class="vis-toggle">
            <button
              class={`vis-btn ${isPublic() ? "vis-btn-public" : ""}`}
              onClick={() => setIsPublic(true)}
            >
              Public
            </button>
            <button
              class={`vis-btn ${!isPublic() ? "vis-btn-private" : ""}`}
              onClick={() => setIsPublic(false)}
            >
              Private
            </button>
          </div>
          <p class="form-hint">
            {isPublic()
              ? "Anyone can find and join this session."
              : "Hidden, share the code to invite players."}
          </p>
        </div>
      </div>

      {/* ── Actions ────────────────────────────────────────────────────── */}
      <div class="modal-actions">
        <button
          class="btn btn-secondary"
          onClick={() => store.setShowCreateSession(false)}
        >
          Cancel
        </button>
        <button
          class={`btn ${isPublic() ? "btn-primary" : "btn-private"}`}
          onClick={create}
        >
          Create
        </button>
      </div>
    </Modal>
  );
}
