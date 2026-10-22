interface NetworkIndicatorProps {
  enabled: boolean;
  onToggle?: (enabled: boolean) => void;
}

/**
 * The single place that reports internet state. The value comes from the Rust
 * network policy, so the UI never claims access is off unless the core agrees.
 */
export function NetworkIndicator({ enabled, onToggle }: NetworkIndicatorProps) {
  const label = enabled ? "Internet on" : "Internet off";
  const statusClass = enabled ? "net net--on" : "net";
  const dotClass = enabled ? "dot dot--attention" : "dot dot--success";

  if (!onToggle) {
    return (
      <span
        className={`${statusClass} net--static`}
        title={enabled ? "Internet access is on for this project" : "Internet access is off for this project"}
      >
        <span className={dotClass} />
        {label}
      </span>
    );
  }

  return (
    <button
      type="button"
      className={statusClass}
      aria-pressed={enabled}
      onClick={() => onToggle(!enabled)}
      title={
        enabled
          ? "Internet is on for this project. Click to turn it off."
          : "Internet is off for this project. Click to turn it on."
      }
    >
      <span className={dotClass} />
      {label}
    </button>
  );
}
