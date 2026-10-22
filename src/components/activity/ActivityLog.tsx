import { useEffect, useState } from "react";
import { ActivityIcon, GlobeIcon } from "../ui/icons";
import { listActivity } from "../../api/activity";
import { formatClock } from "../../util/format";
import type { ActivityEvent, Project } from "../../types";

export function ActivityLog({ project }: { project: Project }) {
  const [events, setEvents] = useState<ActivityEvent[] | null>(null);

  useEffect(() => {
    let cancelled = false;
    setEvents(null);
    listActivity(project.id)
      .then((next) => {
        if (!cancelled) setEvents(next);
      })
      .catch(() => {
        if (!cancelled) setEvents([]);
      });
    return () => {
      cancelled = true;
    };
  }, [project.id]);

  return (
    <div className="activity">
      <div className="activity__inner">
        <div className="panel__head">
          <span className="panel__title">Activity</span>
          <span className="badge">{project.internetEnabled ? "Internet on" : "Offline"}</span>
        </div>

        <p className="muted mt-2">
          A record of what the agent did in this folder. Network rows appear only when you allowed
          internet for this project.
        </p>

        {events === null ? (
          <p className="muted mt-2">
            <span className="spinner" aria-hidden="true" /> Loading activity…
          </p>
        ) : events.length === 0 ? (
          <p className="muted mt-2">Nothing has happened in this folder yet.</p>
        ) : (
          <div className="mt-2">
            {events.map((event) => (
              <div key={event.id} className="activity__row">
                <span className="activity__time">{formatClock(event.at)}</span>
                <span className={`activity__kind activity__kind--${event.kind}`}>
                  {event.kind === "network" ? <GlobeIcon size={12} /> : <ActivityIcon size={12} />}
                  {event.kind === "network" ? "Network" : "Local"}
                </span>
                <span className="activity__summary">
                  {event.summary} <span className="activity__tool">{event.tool}</span>
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
