import { useStore } from "../App";

export default function Toast() {
  const store = useStore();

  return (
    <div class={`toast${store.error() ? " toast-visible" : ""}`}>
      {store.error()}
    </div>
  );
}
