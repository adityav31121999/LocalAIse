import { delay, invoke, isTauri } from "./index";
import { buildMockAnswer, getMockProject, mockChatAppend } from "./mock";
import { subscribeChatTokens } from "../hooks/useStream";
import type { ChatStreamResult } from "../types";
export interface ChatStreamHandlers {
  onToken(token: string): void;
  /** Receives reasoning deltas separately so the UI can keep them tucked away. */
  onReasoning?(delta: string): void;
  signal: AbortSignal;
}

/** Splits text into small chunks so the UI can render a streamed answer. */
function chunkText(text: string): string[] {
  const parts = text.split(/(\s+)/);
  const chunks: string[] = [];
  for (let i = 0; i < parts.length; i += 3) {
    chunks.push(parts.slice(i, i + 3).join(""));
  }
  return chunks;
}

/**
 * Sends a prompt and streams the answer back token by token.
 * Resolves with the authoritative full result once the stream finishes.
 *
 * `sessionId` continues a saved conversation; `null` starts a brand-new chat,
 * which the core answers from a fresh cache. Either way the turn is stored in
 * the project's `chats/` folder.
 *
 * In the packaged app the tokens come from the Rust core (`chat://token`
 * events) as the model generates them; the invoke call still resolves with
 * the complete message and citations, which stays the source of truth.
 */
export interface SamplingParams {
  temperature?: number;
  top_p?: number;
  top_k?: number;
}

export async function streamChat(
  projectId: string,
  prompt: string,
  handlers: ChatStreamHandlers,
  sessionId: string | null,
  sampling?: SamplingParams,
  attachedFiles?: string[],
): Promise<ChatStreamResult> {
  if (isTauri) {
    let streamed = "";
    let reasoning = "";
    const unlisten = await subscribeChatTokens((event) => {
      if (event.projectId !== projectId || handlers.signal.aborted) return;
      if (event.reasoning) {
        reasoning += event.delta;
        handlers.onReasoning?.(event.delta);
        return;
      }
      streamed += event.delta;
      handlers.onToken(event.delta);
    });
    try {
      const result = await invoke<ChatStreamResult>("chat_send", {
        projectId,
        sessionId,
        prompt,
        attached_files: attachedFiles,
        temperature: sampling?.temperature,
        top_p: sampling?.top_p,
        top_k: sampling?.top_k,
      });
      if (handlers.signal.aborted) {
        return { ...result, content: streamed, reasoning };
      }
      return result;
    } finally {
      unlisten();
    }
  }

  const result = buildMockAnswer(getMockProject(projectId), prompt);
  await delay(420);

  let streamed = "";
  let reasoning = "";
  if (result.reasoning) {
    for (const chunk of chunkText(result.reasoning)) {
      if (handlers.signal.aborted) break;
      handlers.onReasoning?.(chunk);
      reasoning += chunk;
      await delay(10);
    }
  }
  for (const chunk of chunkText(result.content)) {
    if (handlers.signal.aborted) break;
    handlers.onToken(chunk);
    streamed += chunk;
    await delay(14);
  }

  // A stopped run keeps only what the user actually saw.
  const content = handlers.signal.aborted ? streamed : result.content;
  const summary = mockChatAppend(
    projectId,
    sessionId,
    prompt,
    content,
    reasoning,
    result.citations,
  );
  return { ...result, sessionId: summary.id, title: summary.title, content, reasoning };
}
