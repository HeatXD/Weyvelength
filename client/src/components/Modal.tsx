import { JSXElement } from "solid-js";
import { X } from "lucide-solid";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: JSXElement;
  class?: string;
}

export function Modal(props: ModalProps) {
  return (
    <div
      class="modal-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div class={`modal${props.class ? ` ${props.class}` : ""}`}>
        <div class="modal-header">
          <span class="modal-title">{props.title}</span>
          <button class="modal-close" onClick={props.onClose} aria-label="Close">
            <X size={13} stroke-width={2.5} />
          </button>
        </div>
        <div class="modal-body">{props.children}</div>
      </div>
    </div>
  );
}
