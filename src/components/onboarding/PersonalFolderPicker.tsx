import { useState } from "react";

export function PersonalFolderPicker({ onFinish, onBack }: { onFinish: () => void; onBack: () => void }) {
  const [folderPath, setFolderPath] = useState("~/Documents/localAIse-Personal");

  return (
    <div style={{ maxWidth: "540px", margin: "40px auto", padding: "24px" }}>
      <h2 style={{ fontSize: "22px", marginBottom: "8px" }}>Set Up Personal Space</h2>
      <p className="muted" style={{ fontSize: "14px", marginBottom: "20px" }}>
        Your personal diary, reminders, and calendar live in this dedicated global directory.
      </p>

      <div style={{ marginBottom: "24px" }}>
        <label style={{ display: "block", fontSize: "13px", fontWeight: 500, marginBottom: "8px" }}>
          Personal Space Directory
        </label>
        <input
          type="text"
          value={folderPath}
          onChange={(e) => setFolderPath(e.target.value)}
          style={{ width: "100%", padding: "8px 12px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)", fontSize: "14px" }}
        />
        <p className="muted" style={{ fontSize: "12px", marginTop: "6px" }}>
          Can be edited later in Settings.
        </p>
      </div>

      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <button className="btn btn--subtle" onClick={onBack}>
          Back
        </button>
        <button className="btn btn--primary" onClick={onFinish} style={{ padding: "8px 20px" }}>
          Complete Setup & Enter Workspace 🚀
        </button>
      </div>
    </div>
  );
}
