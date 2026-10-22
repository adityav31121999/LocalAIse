/** The apply bar and the findings list shared by the runtime tabs.
 *
 * The findings come from the core: an error means the configuration cannot
 * run at all, a warning means it runs but something is worth knowing. The
 * apply button stays disabled while there are errors, so nothing broken can
 * be stored by accident.
 */

import { SCOPE_LABELS, useRuntimeConfig } from "./runtime-config";

export function RuntimeActions() {
  const {
    report,
    messages,
    blocked,
    dirty,
    applying,
    error,
    estimate,
    scope,
    apply,
    reset,
    useRecommended,
  } = useRuntimeConfig();

  const recommended = estimate?.recommended ?? null;
  const showFindings = messages.length > 0;

  return (
    <div className="runtime-actions">
      {showFindings && (
        <ul className="validation" aria-label="What the core found">
          {messages.map((message) => (
            <li key={message.code} className={`validation__item validation__item--${message.severity}`}>
              <span className="validation__title">{message.title}</span>
              <span className="validation__detail">{message.detail}</span>
            </li>
          ))}
        </ul>
      )}

      {error && <p className="perf__error">{error}</p>}

      {report.requiresRestart && !error && (
        <p className="field__hint">
          The running server uses different settings. Applying stores these for {SCOPE_LABELS[scope]}{" "}
          and restarts it — the next chat turn starts it again.
        </p>
      )}

      <div className="perf__actions">
        {recommended && (
          <button type="button" className="btn btn--ghost" disabled={applying} onClick={useRecommended}>
            Use the recommended profile
          </button>
        )}
        {dirty && (
          <button type="button" className="btn btn--ghost" disabled={applying} onClick={reset}>
            Discard changes
          </button>
        )}
        <button
          type="button"
          className="btn btn--primary"
          disabled={applying || blocked}
          onClick={() => void apply()}
        >
          {applying ? "Applying…" : `Apply for ${SCOPE_LABELS[scope]}`}
        </button>
      </div>
    </div>
  );
}
