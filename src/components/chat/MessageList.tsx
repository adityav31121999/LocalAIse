import { useEffect, useRef } from "react";
import { CitationChip } from "./CitationChip";
import { SparkIcon } from "../ui/icons";
import { formatClock, formatRate } from "../../util/format";
import { Markdown } from "../../util/markdown";
import type { ChatMessage } from "../../types";

interface MessageListProps {
  messages: ChatMessage[];
  streaming: boolean;
  /** When false, reasoning traces are hidden even if the model produced them. */
  showThinking: boolean;
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

          return (
            <article key={message.id} className={`msg msg--${message.role}`}>
              <div className={`msg__role${isAssistant ? " msg__role--ai" : ""}`}>
                {isAssistant ? <SparkIcon size={13} /> : null}
                {isAssistant ? "Assistant" : "You"}
                <span className="muted">· {formatClock(message.createdAt)}</span>
              </div>

              {thinking ? (
                <details className="thinking" open={message.pending}>
                  <summary className="thinking__summary">
                    <span
                      className={`thinking__dot${message.pending ? " thinking__dot--live" : ""}`}
                      aria-hidden="true"
                    />
                    Thinking
                  </summary>
                  <div className="thinking__body">{reasoning}</div>
                </details>
              ) : null}

              <div className="msg__body">
                {isAssistant ? (
                  <>
                    <Markdown text={message.content} />
                    {message.pending ? <span className="msg__caret" aria-hidden="true" /> : null}
                  </>
                ) : (
                  <>
                    {message.content}
                    {message.pending ? <span className="msg__caret" aria-hidden="true" /> : null}
                  </>
                )}
              </div>

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
