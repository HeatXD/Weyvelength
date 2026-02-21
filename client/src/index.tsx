/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";

const root = document.getElementById("root") as HTMLElement;
const splash = document.getElementById("splash") as HTMLElement | null;

render(() => <App />, root);

if (splash) {
  const reveal = () => {
    splash.remove();
    root.style.opacity = "1";
  };
  // Delay by one rAF so the browser commits the initial opacity:1 paint
  // before we start the fade-out — otherwise the CSS transition never fires
  // in release builds where the JS bundle executes before the first paint.
  setTimeout(() => {
    requestAnimationFrame(() => {
      splash.style.opacity = "0";
      splash.addEventListener("transitionend", reveal, { once: true });
      setTimeout(reveal, 600);
    });
  }, 800);
} else {
  root.style.opacity = "1";
}
