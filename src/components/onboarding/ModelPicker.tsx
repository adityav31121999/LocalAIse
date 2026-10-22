import { useState } from "react";

const CATALOG_OPTIONS = [
  { id: "qwen-3b", name: "Qwen 2.5 3B Instruct (Q4_K_M)", size: "2.1 GB", tier: "Small (8 GB RAM)" },
  { id: "granite-8b", name: "Granite 4.1 8B Instruct (Q4_K_M)", size: "4.8 GB", tier: "Medium (16 GB RAM)" },
  { id: "gemma-4b", name: "Gemma 4B Instruct (Q4_K_M)", size: "2.9 GB", tier: "Small/Medium" },
];

export function ModelPicker({ onNext, onBack }: { onNext: () => void; onBack: () => void }) {
  const [selected, setSelected] = useState(CATALOG_OPTIONS[1].id);

  return (
    <div style={{ maxWidth: "540px", margin: "40px auto", padding: "24px" }}>
      <h2 style={{ fontSize: "22px", marginBottom: "8px" }}>Select Starter Model</h2>
      <p className="muted" style={{ fontSize: "14px", marginBottom: "20px" }}>
        Approved local quantized models from the catalog. You can also drag and drop any custom GGUF model anytime.
      </p>

      <div style={{ display: "flex", flexDirection: "column", gap: "10px", marginBottom: "24px" }}>
        {CATALOG_OPTIONS.map((opt) => (
          <label
            key={opt.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "12px",
              padding: "12px",
              borderRadius: "8px",
              border: selected === opt.id ? "1.5px solid var(--accent)" : "1px solid var(--border)",
              background: "var(--surface)",
              cursor: "pointer",
            }}
          >
            <input
              type="radio"
              name="model"
              checked={selected === opt.id}
              onChange={() => setSelected(opt.id)}
            />
            <div style={{ flex: 1 }}>
              <div style={{ fontWeight: 600 }}>{opt.name}</div>
              <div className="muted" style={{ fontSize: "12px" }}>
                Target: {opt.tier} · File size: {opt.size}
              </div>
            </div>
          </label>
        ))}
      </div>

      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <button className="btn btn--subtle" onClick={onBack}>
          Back
        </button>
        <button className="btn btn--primary" onClick={onNext}>
          Next: Personal Space →
        </button>
      </div>
    </div>
  );
}
