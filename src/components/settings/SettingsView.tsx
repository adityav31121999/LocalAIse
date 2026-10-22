/** The settings page: Appearance, Models, Inference and Hardware.
 *
 * Appearance stays local (theme/mode/density), Models manages the user's
 * GGUF folders, and the two runtime tabs edit the same inference profile.
 * The runtime tabs share one draft, so switching between them keeps the
 * changes that have not been applied yet.
 *
 * Tabs default back to Appearance on every visit, so nothing persists
 * navigation state unnecessarily.
 */

import { useState } from "react";
import { AppearanceSettings } from "./AppearanceSettings";
import { InferenceSettings } from "./InferenceSettings";
import { HardwareSettings } from "./HardwareSettings";
import { RuntimeConfigProvider } from "./runtime-config";
import { ModelsPanel } from "../models/ModelsPanel";
import { SettingsIcon } from "../ui/icons";

type SettingsTab = "appearance" | "models" | "inference" | "hardware";

const TAB_META: { id: SettingsTab; label: string; title: string; subtitle: string }[] = [
  {
    id: "appearance",
    label: "Appearance",
    title: "Appearance",
    subtitle: "Personalize your localAIse workspace.",
  },
  {
    id: "models",
    label: "Models",
    title: "Models",
    subtitle: "Your GGUF folders and the models ready to load.",
  },
  {
    id: "inference",
    label: "Inference",
    title: "Inference",
    subtitle: "Context, cache and threads — and what they cost in memory.",
  },
  {
    id: "hardware",
    label: "Hardware",
    title: "Hardware",
    subtitle: "Where the model runs: the device, the build and the memory.",
  },
];

export function SettingsView() {
  const [tab, setTab] = useState<SettingsTab>("appearance");
  const active = TAB_META.find((item) => item.id === tab) ?? TAB_META[0];
  const runtimeTab = tab === "inference" || tab === "hardware";

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

        {/* The provider keeps the draft alive across the two runtime tabs.
            It stays mounted even when another tab is open, so an unsaved
            change is not thrown away by looking at the models. */}
        <RuntimeConfigProvider active={runtimeTab}>
          <div id={`settings-panel-${active.id}`} role="tabpanel" className="settings__panel">
            {tab === "appearance" && <AppearanceSettings />}
            {tab === "models" && <ModelsPanel />}
            {tab === "inference" && <InferenceSettings />}
            {tab === "hardware" && <HardwareSettings />}
          </div>
        </RuntimeConfigProvider>
      </div>
    </div>
  );
}
