/** A download in flight: how much of the archive has landed on disk.
 *
 * The installer reports bytes as it writes them. While the server has not yet
 * said how big the file is (or the stage is not the download itself) the bar
 * slides instead of filling, so the page never looks frozen.
 */

import { formatBytes } from "../../util/format";

export function DownloadProgress({
  done,
  total,
  pendingLabel = "Waiting for the download to start…",
}: {
  done: number;
  total: number;
  /** What to say while the total size is not known yet. */
  pendingLabel?: string;
}) {
  const known = total > 0;
  const percent = known ? Math.min(100, Math.round((done / total) * 100)) : null;

  return (
    <div className="download-progress">
      <div
        className="progress"
        role="progressbar"
        aria-label="Download progress"
        aria-valuemin={0}
        {...(known ? { "aria-valuemax": total, "aria-valuenow": done } : {})}
      >
        <div
          className={`progress__bar${known ? "" : " progress__bar--indeterminate"}`}
          style={known ? { width: `${percent}%` } : undefined}
        />
      </div>
      <span className="download-progress__detail">
        {known
          ? `${formatBytes(done)} of ${formatBytes(total)} · ${percent}%`
          : pendingLabel}
      </span>
    </div>
  );
}
