/** Model & Runtime: models on the left, inference + hardware settings on the right.
 *
 * The left panel lists every installed model and offers Load / Unload for the
 * runtime. The right panel holds the inference and hardware settings behind
 * tabs, sharing one RuntimeConfigProvider so the draft, the live estimate and
 * the apply bar stay in sync.
 */

import { useState } from "react";
import { RuntimeConfigProvider } from "./runtime-config";
import { ModelsPanel } from "../models/ModelsPanel";
import { InferenceSettings } from "./InferenceSettings";
import { HardwareSettings } from "./HardwareSettings";

type SettingsTab = "inference" | "hardware";

const TABS: { id: SettingsTab; label: string }[] = [
  { id: "inference", label: "Inference" },
  { id: "hardware", label: "Hardware" },
];

export function ModelConfig() {
  const [tab, setTab] = useState<SettingsTab>("inference");

  return (
    <div className="model-config">
      <div className="model-config__main">
        <div className="model-config__column model-config__column--models">
          <h2 className="model-config__column-title">Models</h2>
          <ModelsPanel />
        </div>

        <div className="model-config__column model-config__column--runtime">
          <div className="model-config__runtime-tabs" role="tablist" aria-label="Settings">
            {TABS.map((item) => (
              <button
                key={item.id}
                type="button"
                role="tab"
                aria-selected={tab === item.id}
                aria-controls={`model-config-panel-${item.id}`}
                className="model-config__runtime-tab"
                onClick={() => setTab(item.id)}
              >
                {item.label}
              </button>
            ))}
          </div>
          <RuntimeConfigProvider active={true}>
            <div className="model-config__runtime-panel">
              {tab === "inference" ? <InferenceSettings /> : <HardwareSettings />}
            </div>
          </RuntimeConfigProvider>
        </div>
      </div>
    </div>
  );
}
