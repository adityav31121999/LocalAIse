import type { SourceStatus } from "../../types";

const STATUS_LABELS: Record<SourceStatus, string> = {
  indexed: "Indexed",
  indexing: "Indexing",
  pending: "Queued",
  error: "Error",
};

export function IndexStatus({ status }: { status: SourceStatus }) {
  return (
    <span className={`file-row__status file-row__status--${status}`}>
      {STATUS_LABELS[status]}
    </span>
  );
}
