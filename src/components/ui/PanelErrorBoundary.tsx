/** Error Boundary — keeps a crash inside one panel from unmounting the whole
 * app. React unmounts the entire tree when a render-phase error escapes a
 * component with no boundary above it, which presented as the whole window
 * going blank while a document was open. */

import { Component, type ErrorInfo, type ReactNode } from "react";
import { DocIcon } from "./icons";

interface PanelErrorBoundaryProps {
  /** What crashed, for the message ("document viewer"). */
  label: string;
  children: ReactNode;
}

interface PanelErrorBoundaryState {
  error: Error | null;
}

export class PanelErrorBoundary extends Component<
  PanelErrorBoundaryProps,
  PanelErrorBoundaryState
> {
  override state: PanelErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): PanelErrorBoundaryState {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`PanelErrorBoundary caught an error in ${this.props.label}:`, error, info.componentStack);
  }

  override render() {
    if (!this.state.error) return this.props.children;
    return (
      <div style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        height: "100%",
        color: "#ef4444",
        textAlign: "center",
        padding: "24px",
        gap: "4px",
      }}>
        <DocIcon size={32} style={{ color: "#ef4444", marginBottom: "8px" }} />
        <h3 style={{ fontSize: "14px", fontWeight: "600", marginBottom: "8px" }}>
          {this.props.label} crashed
        </h3>
        <p style={{ fontSize: "12px", color: "var(--text-muted)", maxWidth: "360px" }}>
          {this.state.error.message}
        </p>
        <button
          type="button"
          className="btn btn--subtle btn--sm"
          onClick={() => this.setState({ error: null })}
          style={{ marginTop: "10px" }}
        >
          Try again
        </button>
      </div>
    );
  }
}
