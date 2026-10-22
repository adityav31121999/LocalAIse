import { useEffect, useLayoutEffect, useState } from "react";
import { useSettingsStore } from "../state/settingsStore";

export type ResolvedTheme = "dark" | "light";

function readSystemTheme(): ResolvedTheme {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** Resolves the stored color mode against the OS setting. */
export function useResolvedMode(): ResolvedTheme {
  const colorMode = useSettingsStore((state) => state.colorMode);
  const [system, setSystem] = useState<ResolvedTheme>(readSystemTheme);

  useEffect(() => {
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystem(query.matches ? "dark" : "light");
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  return colorMode === "system" ? system : colorMode;
}

/**
 * Publishes theme, color mode and density to <html> so the CSS variable layers
 * in styles/themes and styles/modes can resolve. Applied in a layout effect to
 * avoid a flash of the wrong appearance on startup.
 */
export function useTheme(): ResolvedTheme {
  const resolved = useResolvedMode();
  const themeName = useSettingsStore((state) => state.themeName);
  const density = useSettingsStore((state) => state.density);

  useLayoutEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = themeName;
    root.dataset.mode = resolved;
    root.dataset.density = density;
  }, [themeName, resolved, density]);

  return resolved;
}
