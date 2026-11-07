import { useEffect, useRef } from "react";
import { CitationChip } from "./CitationChip";
import { SparkIcon } from "../ui/icons";
import { formatClock, formatRate } from "../../util/format";
import { Markdown } from "../../util/markdown";
import type { AgentStep, ChatMessage } from "../../types";

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
  showThinking,
  pending,
}: {
  steps: AgentStep[];
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
          return (
            <div
              key={step.id}
              className={`agent-tool agent-tool--${tool.status}`}
              title={tool.summary || undefined}
            >
              <span className="agent-tool__icon" aria-hidden="true">
                🔧
              </span>
              <span className="agent-tool__name">{tool.name}</span>
              {tool.input ? <code className="agent-tool__input">{tool.input}</code> : null}
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
                  <Markdown text={message.content} streaming={message.pending ?? false} />
                  {message.pending ? <span className="msg__caret" aria-hidden="true" /> : null}
                </div>
              )}

              {message.citations && message.citations.length > 0 ? (
                <div className="citations">
                  {message.citations.map((citation) => (
                    <CitationChip key={citation.index} citation={citation} />
                  ))}
                </div>
              ) : null}

              {isAssistant && !message.pending && (message.tokensPerSecond ?? 0) > 0 ? (
                <div className="msg__rate">
                  <span className="msg__rate-value" title="Generation speed">
                    {formatRate(message.tokensPerSecond ?? 0)} tok/s
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
