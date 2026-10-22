import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { useTheme } from "./hooks/useTheme";
import { useProjectStore } from "./state/projectStore";
import { useRuntimeStore } from "./state/runtimeStore";
import "./styles/globals.css";

function Root() {
  useTheme();

  useEffect(() => {
    void useProjectStore.getState().loadProjects();
    void useRuntimeStore.getState().load();
  }, []);

  return <App />;
}

const container = document.getElementById("root");
if (!container) throw new Error("Root element #root is missing from index.html");

createRoot(container).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
