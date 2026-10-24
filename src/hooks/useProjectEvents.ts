/** Keeps the sources panel in sync with the background folder watcher.
 *
 * The Rust core emits `sources://updated` whenever its 15-second scan
 * re-indexes changed files, so the UI refreshes without the user pressing
 * "Index". Does nothing when no project is open, and is inert in browser
 * mode (no Rust watcher exists there).
 */

import { useEffect, useState } from "react";
import { isTauri } from "../api/index";
import { listSources } from "../api/projects";
import { useProjectStore } from "../state/projectStore";

export interface SourcesUpdatedEvent {
  projectId: string;
  path: string;
}

function subscribeSourcesUpdated(
  onUpdate: (event: SourcesUpdatedEvent) => void,
): Promise<() => void> {
  if (!isTauri) return Promise.resolve(() => undefined);
  return import("@tauri-apps/api/event")
    .then(({ listen }) =>
      listen<SourcesUpdatedEvent>("sources://updated", (event) => {
        if (event.payload) onUpdate(event.payload);
      }),
    )
    .catch(() => () => undefined);
}

/** Subscribes to `sources://updated` and re-fetches the open project's
 * sources after each event.
 *
 * Returns true while a watcher-triggered refresh is in flight, so the
 * sidebar can show quiet activity instead of stale data.
 */
export function useProjectEvents(projectId: string | null): boolean {
  const [refreshing, setRefreshing] = useState(false);

  useEffect(() => {
    if (!projectId || !isTauri) return;

    let active = true;
    const subscription = subscribeSourcesUpdated((event) => {
      if (event.projectId !== projectId || !active) return;
      setRefreshing(true);
      listSources(event.projectId)
        .then((sources) => {
          // Only replace the list if this is still the open project.
          if (active && useProjectStore.getState().activeProjectId === event.projectId) {
            useProjectStore.setState({ sources });
          }
        })
        .catch(() => undefined)
        .finally(() => {
          if (active) setRefreshing(false);
        });
    });

    return () => {
      active = false;
      void subscription.then((unsubscribe) => unsubscribe());
    };
    // Re-subscribes when the open project changes.
  }, [projectId]);

  return refreshing;
}
