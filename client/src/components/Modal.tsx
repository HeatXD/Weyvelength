import { JSXElement } from "solid-js";

interface ModalProps {
  title: string;
  onClose: () => void;
  children: JSXElement;
}

export function Modal(props: ModalProps) {
  return (
    <div
      class="modal-overlay"
      onClick={(e) => {
        if (e.target === e.currentTarget) props.onClose();
      }}
    >
      <div class="modal">
        <h2 class="modal-title">{props.title}</h2>
        {props.children}
      </div>
    </div>
  );
}
