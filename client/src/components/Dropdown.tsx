import { createSignal, For, Show, onCleanup } from "solid-js";
import "../styles/Dropdown.css";

export interface DropdownOption<T> {
  label: string;
  value: T;
}

interface DropdownProps<T> {
  options: DropdownOption<T>[];
  value: T;
  onChange: (value: T) => void;
  placeholder?: string;
  disabled?: boolean;
}

export function Dropdown<T>(props: DropdownProps<T>) {
  const [open, setOpen] = createSignal(false);

  const selectedLabel = () =>
    props.options.find((o) => o.value === props.value)?.label ??
    props.placeholder ??
    "";

  function pick(value: T) {
    props.onChange(value);
    setOpen(false);
  }

  function onDocClick(e: MouseEvent) {
    if (!(e.target as HTMLElement).closest(".dropdown")) setOpen(false);
  }

  document.addEventListener("click", onDocClick);
  onCleanup(() => document.removeEventListener("click", onDocClick));

  return (
    <div class="dropdown">
      <button class="dropdown-btn" onClick={() => !props.disabled && setOpen((o) => !o)} disabled={props.disabled}>
        <span class="dropdown-value">{selectedLabel()}</span>
        <span class="dropdown-arrow">▾</span>
      </button>
      <Show when={open()}>
        <div class="dropdown-menu">
          <For each={props.options}>
            {(opt) => (
              <div
                class={`dropdown-item${opt.value === props.value ? " selected" : ""}`}
                onClick={() => pick(opt.value)}
              >
                {opt.label}
              </div>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}
