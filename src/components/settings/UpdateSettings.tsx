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
      {/* Auto-check toggle */}
      <section className="field" aria-labelledby="field-auto-update">
        <div className="field__row">
          <div>
            <h2 className="field__label" id="field-auto-update">
              Automatic update checks
            </h2>
            <p className="field__hint">
              LocalAIse contacts GitHub once per day (when online) to check for new versions. No machine
              identifiers are sent — only a generic User-Agent header.
            </p>
          </div>
          <label className="toggle toggle--inline">
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
        </div>
      </section>

      {/* Status strip: manual check + last check */}
      <section className="field field--compact" aria-labelledby="field-status">
        <div className="status-strip">
          <div className="status-card status-card--action">
            <h3 className="status-card__title" id="field-manual-check">
              Manual check
            </h3>
            <p className="status-card__hint">
              Check immediately, bypassing the 24-hour rule. Respects GitHub rate limits.
            </p>
            <button
              type="button"
              className="btn btn--primary btn--sm"
              onClick={handleCheckNow}
              disabled={checking}
              aria-busy={checking}
            >
              {checking ? (
                <>
                  <RefreshCwIcon size={14} className="spin" />
                  Checking…
                </>
              ) : (
                <>
                  <DownloadIcon size={14} />
                  Check now
                </>
              )}
            </button>
          </div>
          <div className="status-card status-card--info">
            <h3 className="status-card__title" id="field-last-check">
              Last check
            </h3>
            <div className="status-card__value">
              <InfoIcon size={14} />
              <span>{formatTimestamp(lastCheck)}</span>
            </div>
            <p className="status-card__hint">
              Updates are only checked once per 24 hours.
            </p>
          </div>
        </div>
      </section>

      {updateInfo && (
        <section className="field field--update field--highlight" aria-labelledby="field-update-available">
          <div className="update-card">
            <div className="update-card__head">
              <CheckCircleIcon size={18} className="text-success" />
              <div>
                <h2 className="field__label" id="field-update-available">
                  Update available
                </h2>
                <span className="update-card__version">Version {updateInfo.version}</span>
              </div>
            </div>
            <div className="update-card__date">
              Released {new Date(updateInfo.date).toLocaleDateString()}
            </div>
            {updateInfo.body && (
              <div className="update-card__notes">{updateInfo.body}</div>
            )}
            <div className="field__actions">
              <button type="button" className="btn btn--primary">
                <DownloadIcon size={14} />
                Download and install
              </button>
              <button type="button" className="btn btn--ghost btn--sm">
                <XCircleIcon size={14} />
                Skip this version
              </button>
            </div>
          </div>
        </section>
      )}

      {error && !updateInfo && (
        <section className="field field--error field--compact" aria-labelledby="field-error">
          <div className="error-card">
            <div className="error-card__head">
              <AlertCircleIcon size={16} className="text-error" />
              <span className="error-card__title">{checking ? "Checking…" : "Error"}</span>
            </div>
            <p className="error-card__message">{error}</p>
          </div>
        </section>
      )}

      <div className="updates-note">
        <InfoIcon size={13} />
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
