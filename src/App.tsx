import { useState } from "react";
import { AppShell } from "./components/layout/AppShell";
import { HomeDashboard } from "./components/dashboard/HomeDashboard";
import { ProjectDashboard } from "./components/dashboard/ProjectDashboard";
import { DiaryDashboard } from "./components/personal/DiaryDashboard";
import { SettingsView } from "./components/settings/SettingsView";
import { DeepResearchView } from "./components/research/DeepResearchView";
import { ThemeSettingsPanel } from "./components/settings/ThemeSettingsPanel";
import { ModelConfigPanel } from "./components/settings/ModelConfigPanel";
import { Welcome } from "./components/onboarding/Welcome";
import { HardwareCheck } from "./components/onboarding/HardwareCheck";
import { ModelPicker } from "./components/onboarding/ModelPicker";
import { PersonalFolderPicker } from "./components/onboarding/PersonalFolderPicker";
import { useProjectStore, selectActiveProject } from "./state/projectStore";
import { useNavStore } from "./state/navStore";
import { useSettingsStore } from "./state/settingsStore";

export default function App() {
  const view = useNavStore((state) => state.view);
  const closeThemeSettings = useNavStore((state) => state.closeThemeSettings);
  const project = useProjectStore(selectActiveProject);
  const onboardingComplete = useSettingsStore((state) => state.onboardingComplete);
  const completeOnboarding = useSettingsStore((state) => state.completeOnboarding);
  const [onboardingStep, setOnboardingStep] = useState(0);

  if (!onboardingComplete) {
    return (
      <div style={{ minHeight: "100vh", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--app-bg)" }}>
        {onboardingStep === 0 && (
          <Welcome onNext={() => setOnboardingStep(1)} onSkip={completeOnboarding} />
        )}
        {onboardingStep === 1 && (
          <HardwareCheck onNext={() => setOnboardingStep(2)} onBack={() => setOnboardingStep(0)} />
        )}
        {onboardingStep === 2 && (
          <ModelPicker onNext={() => setOnboardingStep(3)} onBack={() => setOnboardingStep(1)} />
        )}
        {onboardingStep === 3 && (
          <PersonalFolderPicker onFinish={completeOnboarding} onBack={() => setOnboardingStep(2)} />
        )}
      </div>
    );
  }

  const showThemePanel = view === "theme-settings";
  const showModelConfig = view === "model-config";

  return (
    <AppShell>
      {showModelConfig ? (
        <ModelConfigPanel />
      ) : view === "settings" ? (
        <SettingsView />
      ) : view === "deep-research" ? (
        <DeepResearchView />
      ) : view === "diary" ? (
        <DiaryDashboard />
      ) : project ? (
        <ProjectDashboard key={project.id} project={project} />
      ) : (
        <HomeDashboard />
      )}

      {showThemePanel && (
        <ThemeSettingsPanel onClose={closeThemeSettings} />
      )}
    </AppShell>
  );
}
