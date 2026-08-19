import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { ErrorBoundary } from "./app/ErrorBoundary";
import { FuturesWindow } from "./features/futures/FuturesWindow";
import "./app/reset.css";
import "./app/theme.css";

// One bundle serves both windows; the backend picks which root to mount via the URL it opens.
// Anything other than the known values is the pill — a mistyped parameter must not leave the
// widget with a blank window.
const isFuturesWindow = new URLSearchParams(window.location.search).get("window") === "futures";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>{isFuturesWindow ? <FuturesWindow /> : <App />}</ErrorBoundary>
  </React.StrictMode>,
);
