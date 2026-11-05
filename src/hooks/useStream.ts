/** Subscribes to streaming chat tokens emitted by the Rust core.
 *
 * The backend sends `chat://token` events while a chat turn generates, so the
 * UI can render an answer as it is produced instead of waiting for the full
 * result. Falls back to a no-op when not running inside Tauri (browser dev
 * mode), where `api/chat.ts` fakes the token stream locally.
 *
 * Returns a promise that resolves to the unsubscribe function.
 */

import { isTauri, listen } from "../api/index";
import type { AgentStreamEvent } from "../types";

export interface ChatTokenEvent {
  projectId: string;
  delta: string;
  /** True when the delta belongs to the reasoning trace, not the answer. */
  reasoning?: boolean;
  /** The model call this delta belongs to (agent turns only). */
  call?: number;
}

export interface AgentStreamEnvelope {
  projectId: string;
  event: AgentStreamEvent;
}

type Unsubscribe = () => void;

export function subscribeChatTokens(onToken: (event: ChatTokenEvent) => void): Promise<Unsubscribe> {
  if (!isTauri) return Promise.resolve(() => undefined);

  return listen<ChatTokenEvent>("chat://token", (event) => onToken(event)).catch(() => () => undefined);
}

/** Subscribes to agent-only tool events (call started, call finished). */
export function subscribeAgentEvents(
  onEvent: (event: AgentStreamEnvelope) => void,
): Promise<Unsubscribe> {
  if (!isTauri) return Promise.resolve(() => undefined);

  return listen<AgentStreamEnvelope>("chat://agent", (event) => onEvent(event)).catch(
    () => () => undefined,
  );
}
