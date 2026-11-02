import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DownloadIcon, XIcon, ClockIcon, AlertCircleIcon, CheckCircleIcon } from "../ui/icons";

interface UpdateInfo {
  version: string;
  body: string;
  date: string;
}

interface DownloadProgress {
  progress: number;
  downloaded: number;
  total: number;
}

export function UpdateBanner({ onClose }: { onClose: () => void }) {
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [skipped, setSkipped] = useState(false);

  useEffect(() => {
    // Listen for update-available event from backend
    let unlisten: () => void;
    listen<UpdateInfo>("update-available", (event) => {
      setUpdate(event.payload);
    }).then((fn) => { unlisten = fn; });

    // Also check for pending update on mount
    invoke<UpdateInfo | null>("get_update_status").then((u) => {
      if (u) setUpdate(u);
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleDownload = async () => {
    if (!update) return;
    setDownloading(true);
    setError(null);
    setProgress({ progress: 0, downloaded: 0, total: 0 });

    try {
      // Listen for progress events
      let _unlisten: (() => void) | null = null;
      await listen<DownloadProgress>("download-progress", (event) => {
        setProgress(event.payload);
      }).then((fn) => { _unlisten = fn; });
      // _unlisten is intentionally unused - listener stays active until component unmounts
      void _unlisten;

      await invoke("install_update");

      // If we get here, install succeeded and app will restart
      setProgress({ progress: 100, downloaded: 0, total: 0 });
    } catch (e) {
      setError(e instanceof Error ? e.message : "Download or install failed");
      setDownloading(false);
    }
  };

  const handleSkip = async () => {
    if (!update) return;
    try {
      await invoke("skip_version", { version: update.version });
      setSkipped(true);
      // Auto-close after a moment
      setTimeout(onClose, 1500);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to skip version");
    }
  };

  const handleLater = () => {
    onClose();
  };

  if (!update || skipped) return null;

  return (
    <div className="update-banner" role="status" aria-live="polite">
      <div className="update-banner__content">
        <div className="update-banner__icon">
          <CheckCircleIcon size={24} className="text-success" />
        </div>
        <div className="update-banner__info">
          <h3 className="update-banner__title">Update available</h3>
          <p className="update-banner__version">Version {update.version} is ready to install</p>
          <p className="update-banner__date">Released {new Date(update.date).toLocaleDateString()}</p>
          {update.body && (
            <details className="update-banner__notes">
              <summary>Release notes</summary>
              <div className="update-banner__notes-body">{update.body}</div>
            </details>
          )}
        </div>
      </div>

      {downloading && progress && (
        <div className="update-banner__progress">
          <div className="progress-bar" role="progressbar" aria-valuenow={progress.progress} aria-valuemin={0} aria-valuemax={100}>
            <div className="progress-bar__fill" style={{ width: `${progress.progress}%` }} />
          </div>
          <span className="progress-text">
            {progress.progress}% ({formatBytes(progress.downloaded)} / {formatBytes(progress.total)})
          </span>
        </div>
      )}

      {error && (
        <div className="update-banner__error">
          <AlertCircleIcon size={16} />
          <span>{error}</span>
        </div>
      )}

      <div className="update-banner__actions">
        {downloading ? (
          <button type="button" className="btn btn--ghost" disabled>
            <ClockIcon size={16} className="spin" />
            Downloading…
          </button>
        ) : (
          <>
            <button type="button" className="btn btn--primary" onClick={handleDownload}>
              <DownloadIcon size={16} />
              Download and install
            </button>
            <button type="button" className="btn btn--ghost" onClick={handleSkip}>
              <XIcon size={16} />
              Skip this version
            </button>
            <button type="button" className="btn btn--ghost" onClick={handleLater}>
              Remind me later
            </button>
          </>
        )}
      </div>

      <button
        type="button"
        className="update-banner__close"
        onClick={onClose}
        aria-label="Dismiss"
      >
        <XIcon size={16} />
      </button>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}