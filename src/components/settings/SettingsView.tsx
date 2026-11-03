/** The settings page: Appearance, Model & Runtime, and Updates.
 *
 * Appearance stays local (theme/mode/density). The Model & Runtime tab
 * combines models, inference, and hardware into a single full-width panel
 * so everything is visible at once. The Updates tab controls auto-check
 * and shows update status.
 */

import { useState, useEffect } from "react";
import { AppearanceSettings } from "./AppearanceSettings";
import { UpdateSettings } from "./UpdateSettings";
import { ChevronLeftIcon, SettingsIcon } from "../ui/icons";
import { ModelConfig } from "./ModelConfig";
import { useNavStore } from "../../state/navStore";

type SettingsTab = "appearance" | "model-runtime" | "updates";

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
  {
    id: "updates",
    label: "Updates",
    title: "Updates",
    subtitle: "Configure automatic update checks and view update status.",
  },
];

export function SettingsView() {
  const settingsTab = useNavStore((state) => state.settingsTab);
  const [tab, setTab] = useState<SettingsTab>(settingsTab);
  const active = TAB_META.find((item) => item.id === tab) ?? TAB_META[0];
  const showHome = useNavStore((state) => state.showHome);

  // Sync with navStore if it changes externally
  useEffect(() => {
    setTab(settingsTab);
  }, [settingsTab]);

  return (
    <div className="settings">
      <header className="settings__bar">
        <button
          type="button"
          className="icon-btn icon-btn--sm"
          onClick={showHome}
          title="Back to work directories"
        >
          <ChevronLeftIcon size={16} />
          <span className="sr-only">Back</span>
        </button>
        <span className="settings__bar-title">Settings</span>
      </header>

      <div className="settings__inner settings__inner--full">
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

        <div id={`settings-panel-${active.id}`} role="tabpanel" className="settings__panel settings__panel--full">
          {tab === "appearance" && <AppearanceSettings />}
          {tab === "model-runtime" && <ModelConfig />}
          {tab === "updates" && <UpdateSettings />}
        </div>
      </div>
    </div>
  );
}
