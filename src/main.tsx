import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { applyTheme, getTheme } from "./lib/theme";
import "./index.css";

applyTheme(getTheme());

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The window is created hidden with the app background color; show it after the first paint
// so there is no white flash.
if ("__TAURI_INTERNALS__" in window) {
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void getCurrentWindow().show();
      // Dev builds only: how long until the first paint (the budget is under 1 s).
      if (import.meta.env.DEV) console.info(`[loki dev] first paint ${Math.round(performance.now())} ms after page start`);
    });
  });
}
