/** The settings page: Appearance, and Model & Runtime.
 *
 * Appearance stays local (theme/mode/density). The Model & Runtime tab
 * combines models, inference, and hardware into a single full-width panel
 * so everything is visible at once.
 */

import { useState } from "react";
import { AppearanceSettings } from "./AppearanceSettings";
import { InferenceSettings } from "./InferenceSettings";
import { HardwareSettings } from "./HardwareSettings";
import { RuntimeConfigProvider } from "./runtime-config";
import { ModelsPanel } from "../models/ModelsPanel";
import { SettingsIcon } from "../ui/icons";

type SettingsTab = "appearance" | "model-runtime";

const TAB_META: { id: SettingsTab; label: string; title: string; subtitle: string }[] = [
  {
    id: "appearance",
    label: "Appearance",
    title: "Appearance",
    subtitle: "Personalize your localAIse workspace.",
  },
  {
    id: "model-runtime",
    label: "Model & Runtime",
    title: "Model & Runtime",
    subtitle: "Your models, inference settings, and hardware configuration — all in one view.",
  },
];

export function SettingsView() {
  const [tab, setTab] = useState<SettingsTab>("appearance");
  const active = TAB_META.find((item) => item.id === tab) ?? TAB_META[0];

  return (
    <div className="settings">
      <div className="settings__inner">
        <span className="settings__eyebrow">
          <SettingsIcon size={13} />
          Settings
        </span>
        <h1 className="settings__title">{active.title}</h1>
        <p className="settings__subtitle">{active.subtitle}</p>

        <div className="settings__tabs" role="tablist" aria-label="Settings sections">
          {TAB_META.map((item) => (
            <button
              key={item.id}
              type="button"
              role="tab"
              className="settings__tab"
              aria-selected={tab === item.id}
              aria-controls={`settings-panel-${item.id}`}
              onClick={() => setTab(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>

        <div id={`settings-panel-${active.id}`} role="tabpanel" className="settings__panel">
          {tab === "appearance" && <AppearanceSettings />}
          {tab === "model-runtime" && (
            <RuntimeConfigProvider active={true}>
              <div className="model-runtime-panel">
                <ModelsPanel />
                <InferenceSettings />
                <HardwareSettings />
              </div>
            </RuntimeConfigProvider>
          )}
        </div>
      </div>
    </div>
  );
}
