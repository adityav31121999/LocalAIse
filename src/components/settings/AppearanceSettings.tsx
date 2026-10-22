import type { Density, ThemeMode, ThemeName } from "../../types";
import { THEME_ORDER, useSettingsStore } from "../../state/settingsStore";
import { useResolvedMode } from "../../hooks/useTheme";

/** Presentation metadata for each theme skin. The names come from the store. */
const THEMES: Record<
  ThemeName,
  { label: string; palette: string; bestFor: string; swatch: [string, string, string] }
> = {
  atelier: {
    label: "Atelier",
    palette: "Graphite · Muted teal · Cool grey",
    bestFor: "general productivity, projects and chat",
    swatch: ["#192630", "#66d9b5", "#8296a1"],
  },
  paper: {
    label: "Paper",
    palette: "Warm white · Ivory · Forest green",
    bestFor: "the diary, long-form writing and reading papers",
    swatch: ["#faf7f0", "#2f6b3c", "#cbc0a9"],
  },
  "cyber-lab": {
    label: "Cyber Lab",
    palette: "Deep navy · Electric cyan · Blue",
    bestFor: "model inference, research agents and technical work",
    swatch: ["#0f1a2c", "#34d7ff", "#5b9dff"],
  },
  amethyst: {
    label: "Amethyst",
    palette: "Dark plum · Lavender · Soft violet",
    bestFor: "ideation, content writing and personal organization",
    swatch: ["#23192f", "#c9a6ff", "#7fc6f5"],
  },
  arctic: {
    label: "Arctic",
    palette: "Cool white · Slate · Ice blue",
    bestFor: "document comparison, file management and structured research",
    swatch: ["#f4f7fa", "#26709f", "#b7c4d3"],
  },
  evergreen: {
    label: "Evergreen",
    palette: "Forest charcoal · Moss · Sage",
    bestFor: "the diary, knowledge management and extended research",
    swatch: ["#1b231a", "#a3c98a", "#7cc3c8"],
  },
};

const MODES: { value: ThemeMode; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "Use system preference" },
];

const DENSITIES: { value: Density; label: string; text: string }[] = [
  { value: "compact", label: "Compact", text: "More files and content on screen." },
  { value: "comfortable", label: "Comfortable", text: "Balanced spacing." },
  { value: "spacious", label: "Spacious", text: "Larger controls and more breathing room." },
];

export function AppearanceSettings() {
  const colorMode = useSettingsStore((state) => state.colorMode);
  const setColorMode = useSettingsStore((state) => state.setColorMode);
  const themeName = useSettingsStore((state) => state.themeName);
  const setThemeName = useSettingsStore((state) => state.setThemeName);
  const density = useSettingsStore((state) => state.density);
  const setDensity = useSettingsStore((state) => state.setDensity);
  const resolvedMode = useResolvedMode();

  return (
    <>
      <section className="field" aria-labelledby="field-mode">
          <h2 className="field__label" id="field-mode">
            Color mode
          </h2>
          <p className="field__hint">
            Applies to whichever theme is selected. “Use system preference” follows your operating
            system.
          </p>
          <div className="segmented" role="group" aria-labelledby="field-mode">
            {MODES.map((option) => (
              <button
                key={option.value}
                type="button"
                className="segmented__item"
                aria-pressed={colorMode === option.value}
                onClick={() => setColorMode(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>
        </section>

        <section className="field" aria-labelledby="field-theme">
          <h2 className="field__label" id="field-theme">
            Theme
          </h2>
          <p className="field__hint">
            One shared component system with six visual identities. Each theme supports both color
            modes, so any combination works.
          </p>
          <div className="theme-grid" role="group" aria-labelledby="field-theme">
            {THEME_ORDER.map((name) => {
              const theme = THEMES[name];
              return (
                <button
                  key={name}
                  type="button"
                  className="theme-option"
                  aria-pressed={themeName === name}
                  onClick={() => setThemeName(name)}
                >
                  <span className="theme-option__head">
                    <span className="theme-option__name">{theme.label}</span>
                    <span className="swatch" aria-hidden="true">
                      {theme.swatch.map((color) => (
                        <span key={color} style={{ background: color }} />
                      ))}
                    </span>
                  </span>
                  <span className="theme-option__palette">{theme.palette}</span>
                  <span className="theme-option__best">Best for {theme.bestFor}.</span>
                </button>
              );
            })}
          </div>
        </section>

        <section className="field" aria-labelledby="field-density">
          <h2 className="field__label" id="field-density">
            Interface density
          </h2>
          <p className="field__hint">
            Adjusts spacing and control sizes only — never the layout or the available features.
          </p>
          <div className="density-grid" role="group" aria-labelledby="field-density">
            {DENSITIES.map((option) => (
              <button
                key={option.value}
                type="button"
                className="density-option"
                aria-pressed={density === option.value}
                onClick={() => setDensity(option.value)}
              >
                <span className="density-option__name">{option.label}</span>
                <span className="density-option__text">{option.text}</span>
              </button>
            ))}
          </div>
        </section>

        <div className="preview">
          <span className="preview__line">
            Theme: <strong>{THEMES[themeName].label}</strong> · Mode:{" "}
            <strong>{resolvedMode}</strong> · Density: <strong>{density}</strong>
          </span>
          <span className="preview__note">
            Appearance is stored on this machine and applies immediately. It never changes project
            files, model settings or network permissions.
          </span>
        </div>
    </>
  );
}
