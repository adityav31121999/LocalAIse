/** Full-screen Model Configuration panel — uses complete window space in two columns. */

import { useState } from "react";
import { ChevronLeftIcon } from "../ui/icons";
import { ModelsPanel } from "../models/ModelsPanel";
import { InferenceSettings } from "./InferenceSettings";
import { HardwareSettings } from "./HardwareSettings";
import { RuntimeConfigProvider } from "./runtime-config";
import { useNavStore } from "../../state/navStore";
import { useSettingsStore } from "../../state/settingsStore";

export function ModelConfigPanel() {
  const showHome = useNavStore((state) => state.showHome);
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);
  const [activeTab, setActiveTab] = useState<"inference" | "hardware">("inference");

  const handleBack = () => {
    showHome();
  };

  return (
    <div className="model-config">
      {/* Top bar */}
      <header className="model-config__header">
        <button
          type="button"
          className="model-config__back"
          onClick={handleBack}
          title="Back to projects"
          aria-label="Back to projects"
        >
          <ChevronLeftIcon size={20} />
        </button>
        <h1 className="model-config__title">Model Configuration</h1>
        <div className="model-config__spacer" />
        <button
          type="button"
          className="model-config__sidebar-toggle"
          onClick={toggleSidebar}
          aria-pressed={sidebarCollapsed}
          aria-label={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
          title={sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          <ChevronLeftIcon size={18} style={{ transform: sidebarCollapsed ? "rotate(180deg)" : "none" }} />
        </button>
      </header>

      {/* Main content - two columns */}
      <main className="model-config__main">
        {/* Left column - Models */}
        <section className="model-config__column model-config__column--models" aria-labelledby="models-heading">
          <h2 id="models-heading" className="model-config__column-title">Models</h2>
          <ModelsPanel />
        </section>

        {/* Right column - Inference & Hardware */}
        <section className="model-config__column model-config__column--runtime" aria-labelledby="runtime-heading">
          <div className="model-config__runtime-tabs" role="tablist" aria-label="Runtime settings">
            <button
              type="button"
              role="tab"
              className={`model-config__runtime-tab ${activeTab === "inference" ? "model-config__runtime-tab--active" : ""}`}
              aria-selected={activeTab === "inference"}
              aria-controls="inference-panel"
              id="inference-tab"
              onClick={() => setActiveTab("inference")}
            >
              Inference
            </button>
            <button
              type="button"
              role="tab"
              className={`model-config__runtime-tab ${activeTab === "hardware" ? "model-config__runtime-tab--active" : ""}`}
              aria-selected={activeTab === "hardware"}
              aria-controls="hardware-panel"
              id="hardware-tab"
              onClick={() => setActiveTab("hardware")}
            >
              Hardware
            </button>
          </div>

          <RuntimeConfigProvider active={true}>
            <div
              id="inference-panel"
              role="tabpanel"
              className="model-config__runtime-panel"
              aria-labelledby="inference-tab"
              hidden={activeTab !== "inference"}
            >
              <InferenceSettings />
            </div>
            <div
              id="hardware-panel"
              role="tabpanel"
              className="model-config__runtime-panel"
              aria-labelledby="hardware-tab"
              hidden={activeTab !== "hardware"}
            >
              <HardwareSettings />
            </div>
          </RuntimeConfigProvider>
        </section>
      </main>
    </div>
  );
}