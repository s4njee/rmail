/* @refresh reload */
import { render } from "solid-js/web";
import App from "./App";
import { initTelemetry } from "./lib/telemetry";
import "./styles/fonts.css";
import "./styles/tokens.css";
import "./styles/global.css";

// JS error capture + console→log bridge (Roadmap E2.3): must be installed
// before the app renders so errors during first paint are recorded.
initTelemetry();

// The app is not a document: suppress the webview's right-click browser
// context menu (Epic 1.4). Editable elements keep theirs, so Cut/Copy/Paste
// still work in text fields. Devtools are additionally compiled out of
// release builds (see tauri.conf.json).
document.addEventListener("contextmenu", (event) => {
  const target = event.target as HTMLElement | null;
  const inEditable = target?.closest(
    "input, textarea, [contenteditable='true']",
  );
  if (!inEditable) event.preventDefault();
});

// Dropping a file onto the app must never navigate the webview to it (Epic
// 13.3). The composer's drop zone handles its own drops; everywhere else the
// drop is inert.
document.addEventListener("dragover", (event) => event.preventDefault());
document.addEventListener("drop", (event) => event.preventDefault());

render(() => <App />, document.getElementById("root") as HTMLElement);
