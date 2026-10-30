/** Document reading and summarization API. */

import { delay, invoke, isTauri } from "./index";
import type { InferenceProfile } from "../types";

export interface DocumentContent {
  fileName: string;
  filePath: string;
  mimeType: string;
  text: string;
  pageCount?: number;
  sizeBytes: number;
  /** Base64-encoded file bytes for binary files (e.g., PDFs for visual rendering). */
  fileBytesBase64?: string;
}

export interface SummarizationRequest {
  projectId: string;
  filePath: string;
  prompt?: string;
  profile: InferenceProfile;
}

export interface SummarizationResult {
  summary: string;
  tokensUsed: {
    input: number;
    output: number;
  };
}

/** In-flight read_document calls, so concurrent requests for the same file —
 * React StrictMode remounts, double clicks — share one backend parse instead
 * of extracting the text twice. Entries live only while a call is running,
 * so a later change + reopen always gets a fresh read. */
const inflightDocumentReads = new Map<string, Promise<DocumentContent>>();

/** Read a document file and extract its text content. */
export async function readDocument(
  projectId: string,
  filePath: string,
  extractText?: boolean,
): Promise<DocumentContent> {
  // In browser dev mode, always use mock
  if (!isTauri) {
    return readDocumentMock(projectId, filePath);
  }

  const key = `${projectId}:${filePath}:${extractText ?? false}`;
  const existing = inflightDocumentReads.get(key);
  if (existing) return existing;

  const pending = (async () => {
    // In Tauri, call the real command and let errors propagate.PDF text
    // extraction can legitimately take seconds, so there is deliberately no
    // timeout race with a mock fallback here: a slow parse silently swapped in
    // mock metadata (hardcoded page count, text.length as size) and made the
    // viewer show wrong "750 bytes · 5 pages" style headers. Genuine failures
    // are handled by DocumentViewer (it still loads the file bytes for visual
    // rendering).
    const { invoke: call } = await import("@tauri-apps/api/core");
    return call<DocumentContent>("read_document", { projectId, filePath, extract_text: extractText });
  })();

  inflightDocumentReads.set(key, pending);
  try {
    return await pending;
  } finally {
    inflightDocumentReads.delete(key);
  }
}

/** Read a file as raw bytes (base64-encoded). Used for PDF visual rendering. */
export async function readFileBytes(
  projectId: string,
  filePath: string,
): Promise<string> {
  if (!isTauri) {
    throw new Error("File byte reading only available in Tauri");
  }

  const { invoke: call } = await import("@tauri-apps/api/core");
  return call<string>("read_project_file_bytes", { projectId, path: filePath });
}

async function readDocumentMock(
  _projectId: string,
  filePath: string,
): Promise<DocumentContent> {
  await delay(300);

  // Mock content based on file extension
  const ext = filePath.split(".").pop()?.toLowerCase() ?? "";

  const mockContent: Record<string, string> = {
    pdf: `# Document: ${filePath}\n\nThis is a simulated PDF document content. In the real app, this would be extracted using pdfjs-dist or a native PDF parser.\n\n## Chapter 1: Introduction\n\nLocalAIse is a private, local-first AI workspace designed for folders you own. All processing happens on your machine without sending data to external servers.\n\n## Chapter 2: Features\n\n- Local document indexing and search\n- Chat with RAG grounding\n- Model configuration with memory estimates\n- Project-based workspaces\n- Personal diary and calendar\n\n## Chapter 3: Technical Details\n\nThe application uses llama.cpp for local inference, with support for GPU offloading on compatible hardware. The KV cache can be quantized to fit larger contexts in limited memory.`,
    txt: `# Text Document: ${filePath}\n\nThis is a plain text file. Its content is read directly.\n\nLorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.`,
    md: `# Markdown Document: ${filePath}\n\n## Overview\n\nThis markdown file contains structured content with headings, lists, and formatting.\n\n### Features\n\n- **Bold text** and *italic text*\n- Code blocks with syntax highlighting\n- Tables and lists\n\n### Example Code\n\n\`\`\`typescript\nfunction greet(name: string): string {\n  return \`Hello, \${name}!\`;\n}\n\`\`\`\n\n> Blockquote for important notes.`,
    docx: `# Word Document: ${filePath}\n\nThis would be parsed from a .docx file using a document parser.\n\nContent extracted from the Word document would appear here.`,
  };

  const text = mockContent[ext] ?? `File content for ${filePath}\n\n[Binary or unsupported file type: ${ext}]`;

  return {
    fileName: filePath.split("/").pop() ?? filePath,
    filePath,
    mimeType: getMimeType(ext),
    text,
    pageCount: ext === "pdf" ? 5 : undefined,
    sizeBytes: text.length,
  };
}

function getMimeType(ext: string): string {
  const types: Record<string, string> = {
    pdf: "application/pdf",
    txt: "text/plain",
    md: "text/markdown",
    docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    doc: "application/msword",
    csv: "text/csv",
    json: "application/json",
  };
  return types[ext] ?? "application/octet-stream";
}

/** Summarize a document using the configured AI model. */
export async function summarizeDocument(
  request: SummarizationRequest,
): Promise<SummarizationResult> {
  if (isTauri) {
    return invoke<SummarizationResult>("summarize_document", { request });
  }

  await delay(1500);

  // Simulate token usage based on profile settings
  const inputTokens = Math.min(request.profile.maxInputTokens || 4096, 2000);
  const outputTokens = Math.min(request.profile.maxOutputTokens || 1024, 500);

  const mockSummary = `# Summary: ${request.filePath.split("/").pop()}\n\n## Overview\n\nThis document covers the core concepts and implementation details of LocalAIse, a local-first AI workspace application.\n\n## Key Points\n\n1. **Privacy-First Architecture**: All data processing occurs locally on the user's machine. No documents or queries leave the device.\n\n2. **Local Inference**: Uses llama.cpp for running language models locally with support for GPU acceleration.\n\n3. **Document Indexing**: Files in the project folder are parsed and indexed for retrieval-augmented generation (RAG).\n\n4. **Flexible Model Configuration**: Users can configure context windows, KV cache quantization, and shared token budgets.\n\n5. **Project-Based Workspaces**: Each project maintains its own document index, chat history, and settings.\n\n## Technical Details\n\n- Context window: configurable up to model limits\n- KV cache types: f16, Q8, Q5, Q4 for memory optimization\n- Supports CPU and GPU (CUDA/Metal/Vulkan) inference\n- Memory estimates calculated live based on model and hardware\n\n## Conclusion\n\nLocalAIse provides a complete local AI workspace suitable for sensitive documents and offline work.`;

  return {
    summary: mockSummary,
    tokensUsed: {
      input: inputTokens,
      output: outputTokens,
    },
  };
}

/** Save the AI summary as a markdown file with the same name as the source document. */
export async function saveSummaryAsMarkdown(
  projectId: string,
  sourceFilePath: string,
  summary: string,
): Promise<{ savedPath: string; fileName: string }> {
  const sourceName = sourceFilePath.split("/").pop() ?? sourceFilePath;
  const dotIndex = sourceName.lastIndexOf(".");
  const stem = dotIndex > 0 ? sourceName.slice(0, dotIndex) : sourceName;
  const mdFileName = `${stem}.md`;

  const markdownContent = `# AI Summary\n\n**Source:** ${sourceName}\n**Generated:** ${new Date().toISOString()}\n\n---\n\n${summary}`;

  if (isTauri) {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("write_file_content", {
      projectId,
      file_path: `data/summary/${mdFileName}`,
      content: markdownContent,
    });
  } else {
    await delay(300);
    // Browser mock: store in localStorage for persistence simulation
    try {
      const existing = JSON.parse(localStorage.getItem("localaise_summaries") ?? "{}");
      existing[`${projectId}:data/summary/${mdFileName}`] = {
        content: markdownContent,
        savedAt: new Date().toISOString(),
      };
      localStorage.setItem("localaise_summaries", JSON.stringify(existing));
    } catch {
      // localStorage may not be available in all environments
    }
  }

  return {
    savedPath: `data/summary/${mdFileName}`,
    fileName: mdFileName,
  };
}

/** Generate a custom prompt for document analysis. */
export function buildSummarizationPrompt(
  documentText: string,
  customPrompt?: string,
  maxInputTokens: number = 4096,
): string {
  const basePrompt = customPrompt || `Summarize the following document comprehensively. Include key points, main arguments, and important details.`;

  // Truncate document text to fit within token budget (rough estimate: 1 token ≈ 4 chars)
  const maxChars = maxInputTokens * 3; // Leave room for prompt and response
  const truncatedText = documentText.length > maxChars
    ? documentText.slice(0, maxChars) + "\n\n[Document truncated...]"
    : documentText;

  return `${basePrompt}\n\n---\n\n${truncatedText}`;
}