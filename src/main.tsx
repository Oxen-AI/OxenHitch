import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import App from "./App";
import { isTauri, tauriApi } from "./lib/api";
import { preview } from "./lib/preview";
import "./styles.css";

// Outside the desktop shell (plain `pnpm dev` in a browser) the UI runs on a fake backend.
const api = isTauri() ? tauriApi : preview();
if (!isTauri()) document.documentElement.classList.add("browser");

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App api={api} />
  </StrictMode>,
);
