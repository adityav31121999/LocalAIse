import { useState, useEffect } from "react";
import { useSettingsStore } from "../../state/settingsStore";
import { DownloadIcon, RefreshCwIcon, InfoIcon, CheckCircleIcon, XCircleIcon, AlertCircleIcon } from "../ui/icons";

/** Updates settings tab: toggle auto-check, manual check, last-checked display, skip-version. */
export function UpdateSettings() {
  const [autoUpdateCheck, setAutoUpdateCheck] = useState<boolean>(true);
  const [lastCheck, setLastCheck] = useState<number | null>(null);
  const [checking, setChecking] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<{ version: string; date: string; body: string } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const getAutoUpdateCheck = useSettingsStore((s) => s.getAutoUpdateCheck);
  const setAutoUpdateCheckCmd = useSettingsStore((s) => s.setAutoUpdateCheck);
  const getLastCheck = useSettingsStore((s) => s.getLastCheck);

  // Load initial state
  useEffect(() => {
    loadState();
  }, []);

  const loadState = async () => {
    try {
      const [enabled, last] = await Promise.all([
        getAutoUpdateCheck(),
        getLastCheck(),
      ]);
      setAutoUpdateCheck(enabled);
      setLastCheck(last);
    } catch (e) {
      console.error("Failed to load update settings:", e);
    }
  };

  const handleToggle = async (enabled: boolean) => {
    setAutoUpdateCheck(enabled);
    try {
      await setAutoUpdateCheckCmd(enabled);
    } catch (e) {
      setAutoUpdateCheck(!enabled); // Revert on error
      setError("Failed to save setting");
    }
  };

  const handleCheckNow = async () => {
    setChecking(true);
    setError(null);
    setUpdateInfo(null);
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const result = await invoke<{ version: string; date: string; body: string } | null>("check_now");
      if (result) {
        setUpdateInfo(result);
      } else {
        setError("No update available — you're on the latest version.");
        await loadState(); // Refresh last check time
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Update check failed");
    } finally {
      setChecking(false);
    }
  };

  const formatTimestamp = (ts: number | null): string => {
    if (!ts || ts === 0) return "Never";
    const date = new Date(ts * 1000);
    return date.toLocaleString(undefined, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  };

  return (
    <div className="updates-settings">
      <section className="field" aria-labelledby="field-auto-update">
        <h2 className="field__label" id="field-auto-update">
          Automatic update checks
        </h2>
        <p className="field__hint">
          When enabled, LocalAIse contacts GitHub once per day (when online) to check for new versions.
          No machine identifiers are sent — only a generic User-Agent header.
        </p>
        <label className="toggle">
          <input
            type="checkbox"
            checked={autoUpdateCheck}
            onChange={(e) => handleToggle(e.target.checked)}
            aria-label="Check for updates automatically"
          />
          <span className="toggle__slider" />
          <span className="toggle__label">
            Check for updates automatically (once per day when online)
          </span>
        </label>
      </section>

      <section className="field" aria-labelledby="field-manual-check">
        <h2 className="field__label" id="field-manual-check">
          Manual check
        </h2>
        <p className="field__hint">
          Check for updates immediately, bypassing the 24-hour rule. Respects GitHub rate limits.
        </p>
        <div className="field__actions">
          <button
            type="button"
            className="btn btn--primary"
            onClick={handleCheckNow}
            disabled={checking}
            aria-busy={checking}
          >
            {checking ? (
              <>
                <RefreshCwIcon size={16} className="spin" />
                Checking…
              </>
            ) : (
              <>
                <DownloadIcon size={16} />
                Check now
              </>
            )}
          </button>
        </div>
      </section>

      <section className="field" aria-labelledby="field-last-check">
        <h2 className="field__label" id="field-last-check">
          Last check
        </h2>
        <p className="field__hint">
          Timestamp of the last successful update check. Updates are only checked once per 24 hours.
        </p>
        <div className="status-row">
          <InfoIcon size={16} />
          <span className="status-value">{formatTimestamp(lastCheck)}</span>
        </div>
      </section>

      {updateInfo && (
        <section className="field field--highlight" aria-labelledby="field-update-available">
          <h2 className="field__label" id="field-update-available">
            <CheckCircleIcon size={16} className="text-success" />
            Update available
          </h2>
          <div className="update-info">
            <div className="update-info__version">Version {updateInfo.version}</div>
            <div className="update-info__date">Released {new Date(updateInfo.date).toLocaleDateString()}</div>
            {updateInfo.body && (
              <div className="update-info__notes">{updateInfo.body}</div>
            )}
          </div>
          <div className="field__actions">
            <button type="button" className="btn btn--primary">
              <DownloadIcon size={16} />
              Download and install
            </button>
            <button type="button" className="btn btn--ghost">
              <XCircleIcon size={16} />
              Skip this version
            </button>
          </div>
        </section>
      )}

      {error && !updateInfo && (
        <section className="field field--error" aria-labelledby="field-error">
          <h2 className="field__label" id="field-error">
            <AlertCircleIcon size={16} className="text-error" />
            {checking ? "Checking…" : "Error"}
          </h2>
          <p className="error-message">{error}</p>
        </section>
      )}

      <div className="updates-note">
        <InfoIcon size={14} />
        <span>
          Updates are verified with cryptographic signatures. The app will never install an update
          without your explicit confirmation. See the{" "}
          <a href="https://github.com/adityav31121999/LocalAIse#updates" target="_blank" rel="noopener">
            README
          </a>{" "}
          for details.
        </span>
      </div>
    </div>
  );
}