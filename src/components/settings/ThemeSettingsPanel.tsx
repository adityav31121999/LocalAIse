/** Floating theme settings panel — appears as an overlay from the sidebar. */

import { useEffect, useRef } from "react";
import { XIcon, MonitorIcon, MoonIcon, SunIcon, DensityIcon } from "../ui/icons";
import { useSettingsStore, THEME_ORDER, MODE_ORDER, DENSITIES } from "../../state/settingsStore";
import type { ThemeMode, Density } from "../../types";

const THEME_LABELS: Record<string, string> = {
  "atelier": "Atelier",
  "paper": "Paper",
  "cyber-lab": "Cyber Lab",
  "amethyst": "Amethyst",
  "arctic": "Arctic",
  "evergreen": "Evergreen",
};

const MODE_LABELS: Record<ThemeMode, string> = {
  "system": "System",
  "light": "Light",
  "dark": "Dark",
};

const DENSITY_LABELS: Record<Density, string> = {
  "compact": "Compact",
  "comfortable": "Comfortable",
  "spacious": "Spacious",
};

export function ThemeSettingsPanel({ onClose }: { onClose: () => void }) {
  const themeName = useSettingsStore((state) => state.themeName);
  const setThemeName = useSettingsStore((state) => state.setThemeName);
  const colorMode = useSettingsStore((state) => state.colorMode);
  const setColorMode = useSettingsStore((state) => state.setColorMode);
  const density = useSettingsStore((state) => state.density);
  const setDensity = useSettingsStore((state) => state.setDensity);
  const sidebarCollapsed = useSettingsStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useSettingsStore((state) => state.toggleSidebar);

  const panelRef = useRef<HTMLDivElement>(null);

  // Close on outside click
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (panelRef.current && !panelRef.current.contains(event.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [onClose]);

  // Close on Escape key
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  return (
    <div className="theme-panel-overlay" onClick={onClose}>
      <div
        className="theme-panel"
        ref={panelRef}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Theme settings"
      >
        <header className="theme-panel__header">
          <h2 className="theme-panel__title">Theme Settings</h2>
          <button
            type="button"
            className="theme-panel__close"
            onClick={onClose}
            aria-label="Close theme settings"
          >
            <XIcon size={18} />
          </button>
        </header>

        <div className="theme-panel__content">
          {/* Color Theme */}
          <section className="theme-panel__section">
            <h3 className="theme-panel__section-title">Color Theme</h3>
            <div className="theme-panel__options" role="group" aria-label="Color themes">
              {THEME_ORDER.map((theme) => (
                <button
                  key={theme}
                  type="button"
                  className={`theme-panel__option ${themeName === theme ? "theme-panel__option--active" : ""}`}
                  onClick={() => setThemeName(theme)}
                  aria-pressed={themeName === theme}
                >
                  <div className="theme-panel__option-swatch" data-theme={theme} />
                  <span className="theme-panel__option-label">{THEME_LABELS[theme]}</span>
                </button>
              ))}
            </div>
          </section>

          {/* Color Mode */}
          <section className="theme-panel__section">
            <h3 className="theme-panel__section-title">Color Mode</h3>
            <div className="theme-panel__options" role="group" aria-label="Color modes">
              {MODE_ORDER.map((mode: ThemeMode) => (
                <button
                  key={mode}
                  type="button"
                  className={`theme-panel__option ${colorMode === mode ? "theme-panel__option--active" : ""}`}
                  onClick={() => setColorMode(mode)}
                  aria-pressed={colorMode === mode}
                >
                  <span className="theme-panel__option-icon">
                    {mode === "system" && <MonitorIcon size={16} />}
                    {mode === "light" && <SunIcon size={16} />}
                    {mode === "dark" && <MoonIcon size={16} />}
                  </span>
                  <span className="theme-panel__option-label">{MODE_LABELS[mode]}</span>
                </button>
              ))}
            </div>
          </section>

          {/* Density */}
          <section className="theme-panel__section">
            <h3 className="theme-panel__section-title">Density</h3>
            <div className="theme-panel__options" role="group" aria-label="Interface density">
              {DENSITIES.map((d: Density) => (
                <button
                  key={d}
                  type="button"
                  className={`theme-panel__option ${density === d ? "theme-panel__option--active" : ""}`}
                  onClick={() => setDensity(d)}
                  aria-pressed={density === d}
                >
                  <span className="theme-panel__option-icon">
                    <DensityIcon size={16} />
                  </span>
                  <span className="theme-panel__option-label">{DENSITY_LABELS[d]}</span>
                </button>
              ))}
            </div>
          </section>

          {/* Sidebar Toggle */}
          <section className="theme-panel__section">
            <label className="theme-panel__toggle">
              <input
                type="checkbox"
                checked={sidebarCollapsed}
                onChange={toggleSidebar}
              />
              <span className="theme-panel__toggle-label">Collapse sidebar</span>
            </label>
          </section>
        </div>
      </div>
    </div>
  );
}