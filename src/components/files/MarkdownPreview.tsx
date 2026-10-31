/** Renders markdown content as formatted HTML. Used for AI summaries
 * so users get a readable preview alongside the raw .md file. */
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

interface MarkdownPreviewProps {
  markdown: string;
}

export function MarkdownPreview({ markdown }: MarkdownPreviewProps) {
  if (!markdown?.trim()) {
    return null;
  }

  return (
    <ReactMarkdown
      remarkPlugins={[remarkGfm]}
      components={{
        h1: (node: any) => (
          <h1
            style={{
              fontSize: "1.5rem",
              fontWeight: "700",
              margin: "1.5rem 0 0.75rem",
              color: "var(--accent-primary)",
              borderBottom: "2px solid var(--border-strong)",
              paddingBottom: "0.5rem",
            }}
          >
            {node.children}
          </h1>
        ),
        h2: (node: any) => (
          <h2
            style={{
              fontSize: "1.25rem",
              fontWeight: "600",
              margin: "1.25rem 0 0.625rem",
              color: "var(--text)",
            }}
          >
            {node.children}
          </h2>
        ),
        h3: (node: any) => (
          <h3
            style={{
              fontSize: "1.125rem",
              fontWeight: "600",
              margin: "1rem 0 0.5rem",
              color: "var(--text)",
            }}
          >
            {node.children}
          </h3>
        ),
        p: (node: any) => (
          <p
            style={{
              margin: "0.75rem 0",
              lineHeight: 1.7,
              color: "var(--text)",
            }}
          >
            {node.children}
          </p>
        ),
        ul: (node: any) => (
          <ul style={{ margin: "0.75rem 0", paddingLeft: "1.5rem", color: "var(--text)" }}>
            {node.children}
          </ul>
        ),
        ol: (node: any) => (
          <ol style={{ margin: "0.75rem 0", paddingLeft: "1.5rem", color: "var(--text)" }}>
            {node.children}
          </ol>
        ),
        li: (node: any) => (
          <li style={{ margin: "0.375rem 0", color: "var(--text)" }}>{node.children}</li>
        ),
        strong: (node: any) => (
          <strong style={{ color: "var(--text)" }}>{node.children}</strong>
        ),
        code: (node: any) => {
          const inline = !node.data?.derivedFrom;
          return (
            <code
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "0.875rem",
                background: "var(--background-elevated)",
                padding: inline ? "0.125rem 0.375rem" : "0.75rem 1rem",
                borderRadius: "var(--radius-sm)",
                color: "var(--accent-primary)",
                display: inline ? "inline" : "block",
                margin: inline ? 0 : "0.5rem 0",
                whiteSpace: inline ? "pre" : "pre-wrap",
                wordWrap: "break-word",
              }}
            >
              {node.children}
            </code>
          );
        },
        pre: (node: any) => {
          const code = node.children?.[0];
          return (
            <pre
              style={{
                fontFamily: "var(--font-mono)",
                fontSize: "0.875rem",
                background: "var(--background-elevated)",
                padding: "1rem",
                borderRadius: "var(--radius-md)",
                color: "var(--text)",
                overflowX: "auto",
                margin: "1rem 0",
                whiteSpace: "pre-wrap",
                wordWrap: "break-word",
              }}
            >
              {code?.children}
            </pre>
          );
        },
        blockquote: (node: any) => (
          <blockquote
            style={{
              margin: "0.75rem 0",
              paddingLeft: "1rem",
              borderLeft: "4px solid var(--accent-primary)",
              color: "var(--text-muted)",
              fontStyle: "italic",
            }}
          >
            {node.children}
          </blockquote>
        ),
        table: (node: any) => (
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              margin: "1rem 0",
              color: "var(--text)",
            }}
          >
            {node.children}
          </table>
        ),
        thead: (node: any) => (
          <thead>
            {node.children}
          </thead>
        ),
        tbody: (node: any) => (
          <tbody>
            {node.children}
          </tbody>
        ),
        tr: (node: any) => (
          <tr style={{ borderBottom: "1px solid var(--border)" }}>{node.children}</tr>
        ),
        th: (node: any) => (
          <th
            style={{
              textAlign: "left",
              padding: "0.5rem",
              fontWeight: "600",
              borderBottom: "2px solid var(--border-strong)",
              color: "var(--text)",
            }}
          >
            {node.children}
          </th>
        ),
        td: (node: any) => (
          <td style={{ padding: "0.5rem", borderBottom: "1px solid var(--border)" }}>
            {node.children}
          </td>
        ),
        hr: () => (
          <hr
            style={{
              border: "none",
              borderTop: "1px solid var(--border)",
              margin: "1rem 0",
            }}
          />
        ),
        a: (node: any) => (
          <a
            href={node.url}
            target="_blank"
            rel="noopener noreferrer"
            style={{
              color: "var(--accent-primary)",
              textDecoration: "underline",
            }}
          >
            {node.children}
          </a>
        ),
      }}
    >
      {markdown}
    </ReactMarkdown>
  );
}
