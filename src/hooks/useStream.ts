/** Subscribes to streaming chat tokens emitted by the Rust core.
 *
 * The backend sends `chat://token` events while a chat turn generates, so the
 * UI can render an answer as it is produced instead of waiting for the full
 * result. Falls back to a no-op when not running inside Tauri (browser dev
 * mode), where `api/chat.ts` fakes the token stream locally.
 *
 * Returns a promise that resolves to the unsubscribe function.
 */

import { isTauri } from "../api/index";

export interface ChatTokenEvent {
  projectId: string;
  delta: string;
  /** True when the delta belongs to the reasoning trace, not the answer. */
  reasoning?: boolean;
}

type Unsubscribe = () => void;

export function subscribeChatTokens(onToken: (event: ChatTokenEvent) => void): Promise<Unsubscribe> {
  if (!isTauri) return Promise.resolve(() => undefined);

  return import("@tauri-apps/api/event").then(({ listen }) =>
    listen<ChatTokenEvent>("chat://token", (event) => {
      if (event.payload) onToken(event.payload);
    }),
  );
}
