import { useEffect, useRef } from "react";
import { CitationChip } from "./CitationChip";
import { SparkIcon } from "../ui/icons";
import { useChatStore } from "../../state/chatStore";
import { formatClock, formatRate } from "../../util/format";
import { Markdown } from "../../util/markdown";
import type { AgentStep, CallUsage, ChatMessage } from "../../types";

interface MessageListProps {
  messages: ChatMessage[];
  streaming: boolean;
  /** When false, reasoning traces are hidden even if the model produced them. */
  showThinking: boolean;
}

/**
 * Agent turns are a sequence: think, call a tool, think again, answer. Each
 * step keeps its own block so the transcript mirrors what actually happened
 * instead of collapsing every call into one thinking note and one answer.
 */
function AgentTranscript({
  steps,
  callUsages,
  showThinking,
  pending,
}: {
  steps: AgentStep[];
  callUsages?: CallUsage[];
  showThinking: boolean;
  pending: boolean;
}) {
  const lastReasoningIndex = steps.reduce(
    (last, step, index) => (step.kind === "reasoning" ? index : last),
    -1,
  );

  return (
    <div className="agent-steps">
      {steps.map((step, index) => {
        if (step.kind === "reasoning") {
          const text = step.text?.trim() ?? "";
          if (!showThinking || !text) return null;
          const live = pending && index === lastReasoningIndex;
          return (
            <details className="thinking" key={step.id} open={live}>
              <summary className="thinking__summary">
                <span
                  className={`thinking__dot${live ? " thinking__dot--live" : ""}`}
                  aria-hidden="true"
                />
                Thinking
              </summary>
              <div className="thinking__body">
                <Markdown text={text} streaming={pending} />
              </div>
            </details>
          );
        }

        if (step.kind === "tool" && step.tool) {
          const tool = step.tool;
          const usage = callUsages?.find((call) => call.index === step.call);
          // The row keeps its short one-line look; the full call (raw
          // arguments and the result the model read) rides along on the
          // tooltip so the saved transcript stays inspectable.
          const detail = [
            `${tool.name}(${tool.arguments ?? tool.input})`,
            tool.output ? `→ ${tool.output}` : tool.summary ? `→ ${tool.summary}` : "",
          ]
            .filter(Boolean)
            .join("\n");
          return (
            <div
              key={step.id}
              className={`agent-tool agent-tool--${tool.status}`}
              title={detail || undefined}
            >
              <span className="agent-tool__icon" aria-hidden="true">
                🔧
              </span>
              <span className="agent-tool__name">{tool.name}</span>
              {tool.input ? <code className="agent-tool__input">{tool.input}</code> : null}
              {usage ? (
                <span
                  className="agent-tool__tokens"
                  title={`Model call ${step.call + 1}: ${usage.promptTokens.toLocaleString()} prompt + ${usage.completionTokens.toLocaleString()} generated tokens`}
                >
                  {usage.totalTokens.toLocaleString()} tok
                </span>
              ) : null}
              <span className="agent-tool__status">
                {tool.status === "running"
                  ? "running"
                  : tool.status === "error"
                    ? "failed"
                    : "done"}
              </span>
            </div>
          );
        }

        if (step.kind === "content") {
          return (
            <div className="msg__body" key={step.id}>
              <Markdown text={step.text ?? ""} streaming={pending} />
            </div>
          );
        }

        return null;
      })}
      {pending ? <span className="msg__caret" aria-hidden="true" /> : null}
    </div>
  );
}

export function MessageList({ messages, streaming, showThinking }: MessageListProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const streamingProjectId = useChatStore((state) => state.streamingProjectId);
  const pendingRequestsMap = useChatStore((state) => state.pendingRequests);
  const respondToRequest = useChatStore((state) => state.respondToRequest);
  const activeRequests = streamingProjectId ? pendingRequestsMap[streamingProjectId] ?? [] : [];
  const stickToBottom = useRef(true);

  useEffect(() => {
    const element = scrollRef.current;
    if (!element || !stickToBottom.current) return;
    element.scrollTop = element.scrollHeight;
  }, [messages, streaming, showThinking]);

  const handleScroll = () => {
    const element = scrollRef.current;
    if (!element) return;
    stickToBottom.current = element.scrollHeight - element.scrollTop - element.clientHeight < 120;
  };

  return (
    <div className="chat__scroll" ref={scrollRef} onScroll={handleScroll}>
      <div className="chat__column">
        {messages.map((message) => {
          const isAssistant = message.role === "assistant";
          const reasoning = message.reasoning?.trim() ?? "";
          const thinking = isAssistant && showThinking && reasoning.length > 0;
          const agentSteps = isAssistant ? message.steps ?? [] : [];

          return (
            <article key={message.id} className={`msg msg--${message.role}`}>
              <div className={`msg__role${isAssistant ? " msg__role--ai" : ""}`}>
                {isAssistant ? <SparkIcon size={13} /> : null}
                {isAssistant ? "Assistant" : "You"}
                <span className="muted">· {formatClock(message.createdAt)}</span>
              </div>

              {isAssistant ? (
                agentSteps.length > 0 ? (
                  <AgentTranscript
                    steps={agentSteps}
                    callUsages={message.callUsages}
                    showThinking={showThinking}
                    pending={message.pending ?? false}
                  />
                ) : (
                  <>
                    {thinking ? (
                      <details className="thinking" open={message.pending}>
                        <summary className="thinking__summary">
                          <span
                            className={`thinking__dot${message.pending ? " thinking__dot--live" : ""}`}
                            aria-hidden="true"
                          />
                          Thinking
                        </summary>
                        <div className="thinking__body">
                          <Markdown text={reasoning} streaming={message.pending ?? false} />
                        </div>
                      </details>
                    ) : null}

                    <div className="msg__body">
                      <Markdown text={message.content} streaming={message.pending ?? false} />
                      {message.pending ? <span className="msg__caret" aria-hidden="true" /> : null}
                    </div>
                  </>
                )
              ) : (
                <div className="msg__body">
                  <Markdown text={message.content} streaming={message.pending ?? false} mentions />
                  {message.pending ? <span className="msg__caret" aria-hidden="true" /> : null}
                </div>
              )}

              {isAssistant && message.pending && streamingProjectId && activeRequests.length > 0 ? (
                <div className="harness-approvals" style={{ marginTop: 8, display: "flex", flexDirection: "column", gap: 8 }}>
                  {activeRequests.map((req) => (
                    <div
                      key={req.id}
                      className="harness-approval-card"
                      style={{
                        border: "1px solid var(--border, #334155)",
                        borderRadius: 8,
                        padding: "10px 12px",
                        background: "var(--surface-2, rgba(255,255,255,0.03))",
                      }}
                    >
                      <div style={{ fontSize: 13, fontWeight: 600, marginBottom: 4 }}>
                        Approval required: <code>{req.tool}</code> ({req.actionKind})
                      </div>
                      <div style={{ fontSize: 12, opacity: 0.85, marginBottom: 8 }}>{req.summary}</div>
                      <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                        <button
                          type="button"
                          className="btn btn--primary"
                          onClick={() =>
                            void respondToRequest(streamingProjectId, req.id, { type: "approveOnce" })
                          }
                        >
                          Allow once
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={() =>
                            void respondToRequest(streamingProjectId, req.id, { type: "approveAlways" })
                          }
                        >
                          Always allow (session)
                        </button>
                        <button
                          type="button"
                          className="btn"
                          onClick={() =>
                            void respondToRequest(streamingProjectId, req.id, {
                              type: "reject",
                              reason: "Rejected by user.",
                            })
                          }
                        >
                          Reject
                        </button>
                      </div>
                    </div>
                  ))}
                </div>
              ) : null}

              {message.citations && message.citations.length > 0 ? (
                <div className="citations">
                  {message.citations.map((citation) => (
                    <CitationChip key={citation.index} citation={citation} />
                  ))}
                </div>
              ) : null}

              {isAssistant && !message.pending && (message.tokensPerSecond ?? 0) > 0 ? (
                <div
                  className="msg__rate"
                  title={
                    message.callUsages && message.callUsages.length > 1
                      ? message.callUsages
                          .map(
                            (call) =>
                              `call ${call.index + 1}: ${call.promptTokens.toLocaleString()} in / ${call.completionTokens.toLocaleString()} out`,
                          )
                          .join("\n")
                      : undefined
                  }
                >
                  <span className="msg__rate-value" title="Generation speed">
                    {formatRate(message.tokensPerSecond ?? 0)} tok/s
                    {message.callUsages && message.callUsages.length > 1
                      ? ` · ${message.callUsages.length} calls`
                      : ""}
                  </span>
                </div>
              ) : null}
            </article>
          );
        })}
      </div>
    </div>
  );
}
