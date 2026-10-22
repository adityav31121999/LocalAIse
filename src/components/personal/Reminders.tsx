import { useState, useEffect } from "react";
import { listCalendarEvents, createCalendarEvent, deleteCalendarEvent, exportCalendarIcs, importCalendarIcs } from "../../api/personal";
import type { CalendarEvent } from "../../types";

export function Reminders({ selectedDate }: { selectedDate: string | null }) {
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [location, setLocation] = useState("");
  const [importIcsText, setImportIcsText] = useState("");
  const [showImport, setShowImport] = useState(false);
  const [loading, setLoading] = useState(false);

  const loadEvents = async () => {
    try {
      const data = await listCalendarEvents();
      setEvents(data);
    } catch (e) {
      console.error(e);
    }
  };

  useEffect(() => {
    void loadEvents();
  }, []);

  const handleCreate = async () => {
    if (!title.trim()) return;
    const dateStr = selectedDate || new Date().toISOString().slice(0, 10);
    const newEvent: CalendarEvent = {
      id: "",
      title: title.trim(),
      description: description.trim() || undefined,
      startAt: dateStr,
      location: location.trim() || undefined,
    };

    setLoading(true);
    try {
      await createCalendarEvent(newEvent);
      setTitle("");
      setDescription("");
      setLocation("");
      await loadEvents();
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (id: string) => {
    await deleteCalendarEvent(id);
    await loadEvents();
  };

  const handleExport = async () => {
    const ics = await exportCalendarIcs();
    const blob = new Blob([ics], { type: "text/calendar;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "calendar.ics";
    a.click();
    URL.revokeObjectURL(url);
  };

  const handleImport = async () => {
    if (!importIcsText.trim()) return;
    await importCalendarIcs(importIcsText);
    setImportIcsText("");
    setShowImport(false);
    await loadEvents();
  };

  const activeDateEvents = selectedDate
    ? events.filter((e) => e.startAt.startsWith(selectedDate))
    : events;

  return (
    <div style={{ padding: "16px", borderTop: "1px solid var(--border)", marginTop: "16px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: "12px" }}>
        <h4 style={{ margin: 0, fontSize: "14px", fontWeight: 600 }}>
          Calendar Events {selectedDate ? `(${selectedDate})` : ""}
        </h4>
        <div style={{ display: "flex", gap: "8px" }}>
          <button
            className="btn btn--subtle"
            style={{ fontSize: "11px", padding: "2px 8px" }}
            onClick={handleExport}
            title="Export calendar to standard .ics file"
          >
            Export ICS
          </button>
          <button
            className="btn btn--subtle"
            style={{ fontSize: "11px", padding: "2px 8px" }}
            onClick={() => setShowImport(!showImport)}
            title="Import .ics events"
          >
            Import ICS
          </button>
        </div>
      </div>

      {showImport && (
        <div style={{ background: "var(--surface)", border: "1px solid var(--border)", borderRadius: "6px", padding: "10px", marginBottom: "12px" }}>
          <textarea
            placeholder="Paste raw .ics content here..."
            value={importIcsText}
            onChange={(e) => setImportIcsText(e.target.value)}
            style={{ width: "100%", height: "80px", fontSize: "12px", borderRadius: "4px", padding: "6px" }}
          />
          <div style={{ marginTop: "6px", display: "flex", gap: "6px" }}>
            <button className="btn btn--primary" style={{ fontSize: "12px", padding: "4px 10px" }} onClick={handleImport}>
              Import
            </button>
            <button className="btn btn--subtle" style={{ fontSize: "12px", padding: "4px 10px" }} onClick={() => setShowImport(false)}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* New Event Input */}
      <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginBottom: "16px" }}>
        <input
          type="text"
          placeholder="New event title..."
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          style={{ padding: "6px 10px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)", fontSize: "13px" }}
        />
        <div style={{ display: "flex", gap: "8px" }}>
          <input
            type="text"
            placeholder="Location (optional)..."
            value={location}
            onChange={(e) => setLocation(e.target.value)}
            style={{ flex: 1, padding: "6px 10px", borderRadius: "6px", border: "1px solid var(--border)", background: "var(--surface)", fontSize: "12px" }}
          />
          <button
            className="btn btn--primary"
            disabled={loading || !title.trim()}
            onClick={handleCreate}
            style={{ padding: "6px 14px", fontSize: "12px" }}
          >
            Add Event
          </button>
        </div>
      </div>

      {/* Events List */}
      <div style={{ display: "flex", flexDirection: "column", gap: "6px" }}>
        {activeDateEvents.length === 0 ? (
          <p className="muted" style={{ fontSize: "12px", margin: 0 }}>
            No scheduled events for this date.
          </p>
        ) : (
          activeDateEvents.map((evt) => (
            <div
              key={evt.id}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "8px 10px",
                background: "var(--surface)",
                border: "1px solid var(--border)",
                borderRadius: "6px",
                fontSize: "13px",
              }}
            >
              <div>
                <strong>{evt.title}</strong>
                {evt.location && <span className="muted" style={{ marginLeft: "8px", fontSize: "11px" }}>📍 {evt.location}</span>}
              </div>
              <button
                className="icon-btn icon-btn--sm"
                onClick={() => handleDelete(evt.id)}
                title="Delete event"
                style={{ color: "#ff6b6b", cursor: "pointer" }}
              >
                ✕
              </button>
            </div>
          ))
        )}
      </div>
    </div>
  );
}
