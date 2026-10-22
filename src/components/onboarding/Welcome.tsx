export function Welcome({ onNext }: { onNext: () => void }) {
  return (
    <div style={{ maxWidth: "540px", margin: "40px auto", textAlign: "center", padding: "24px" }}>
      <h1 style={{ fontSize: "28px", marginBottom: "12px", color: "var(--text-primary)" }}>
        Welcome to localAIse
      </h1>
      <p style={{ fontSize: "15px", lineHeight: "1.6", color: "var(--text-secondary)", marginBottom: "28px" }}>
        Your private, local-first AI workspace. All your documents, notes, and research remain
        exclusively on your physical device. No cloud accounts, no subscription keys, and internet access
        strictly off by default.
      </p>
      <button className="btn btn--primary" onClick={onNext} style={{ padding: "10px 24px", fontSize: "15px" }}>
        Get Started →
      </button>
    </div>
  );
}
