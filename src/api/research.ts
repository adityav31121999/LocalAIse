import { invoke, isTauri, listen } from "./index";
import { delay } from "./index";

export type ResearchMode = "online" | "offline";

export type ResearchEvent =
  | { type: "Planned"; sub_questions: string[] }
  | { type: "Searching"; query: string }
  | { type: "Fetched"; url: string; tier: string; chars: number }
  | { type: "FactExtracted"; sub_q: number; source_id: number; claim: string }
  | { type: "GapFound"; missing: string[] }
  | { type: "SectionWritten"; title: string; markdown: string; confidence?: number; citation_count?: number }
  | {
      type: "Verified";
      supported: number;
      unsupported: number;
      stage1_passes?: number;
      stage2_supported?: number;
      stage2_contradicted?: number;
      stage2_unsupported?: number;
      /** Stage-2 LLM failures; the fact was kept via stage 1. */
      stage2_failed?: number;
    }
  | { type: "Notice"; message: string }
  | { type: "Done"; report_id: number; report_path?: string }
  | { type: "Error"; message: string };

/** One step of a deep-research plan, mirroring the Rust `PlanStep`. */
export interface DeepResearchPlanStep {
  id: number;
  question: string;
  source: "local" | "internet";
  searchQueries: string[];
  answerCriteria: string;
}

/** The model's plan plus the prompts/raw response, for the audit transcript. */
export interface DeepResearchPlan {
  systemPrompt: string;
  userPrompt: string;
  rawResponse: string;
  steps: DeepResearchPlanStep[];
}

export interface PlanDeepResearchParams {
  question: string;
  mode?: ResearchMode;
  projectId: string;
}

export interface RunDeepResearchParams {
  question: string;
  mode?: ResearchMode;
  projectId: string;
  providedUrls?: string[];
  useBrowser?: boolean;
  budgetPreset?: "quick" | "standard" | "deep";
  /** Reviewed plan from `planDeepResearch`. When omitted the backend plans. */
  plan?: DeepResearchPlan;
}

export type ExportFormat = "markdown" | "json" | "html" | "notion" | "text" | "csljson";
export type CitationStyle = "apa" | "ieee" | "chicago" | "mla" | "harvard" | "numeric";

export interface ExportResearchReportParams {
  projectId: string;
  /** Numeric report id from the `Done` event / `runDeepResearch` return value. */
  reportId: number;
  format: ExportFormat;
  citationStyle?: CitationStyle;
}

// ---------------------------------------------------------------------------
// Browser-dev mock (npm run dev without Tauri): a deterministic fake pipeline
// that exercises the whole UI, events included. Only used when
// `window.__TAURI_INTERNALS__` is absent.
// ---------------------------------------------------------------------------

type MockHandler = (event: ResearchEvent) => void;
const mockHandlers = new Set<MockHandler>();
let mockStopRequested = false;

function mockEmit(event: ResearchEvent) {
  for (const handler of mockHandlers) handler(event);
}

const MOCK_MARKDOWN =
  "## Overview\n\nThis is a mocked deep-research report rendered entirely in the browser dev build.\n\n- Point the app at Tauri (`npm start`) for a real run.";

function mockPlan(question: string): DeepResearchPlan {
  const steps: DeepResearchPlanStep[] = [
    {
      id: 1,
      question: `Background: ${question}`,
      source: "local",
      searchQueries: [question],
      answerCriteria: "Establishes the fundamentals from project documents",
    },
    {
      id: 2,
      question: `Current state: ${question}`,
      source: "internet",
      searchQueries: [question, `${question} latest`],
      answerCriteria: "Recent sources corroborating the key claims",
    },
  ];
  return {
    systemPrompt: "(mock) You are a deep-research planner.",
    userPrompt: `Main question: ${question}`,
    rawResponse: "(mock) {}",
    steps,
  };
}

async function mockRun(params: RunDeepResearchParams): Promise<number> {
  mockStopRequested = false;
  const steps = params.plan?.steps ?? mockPlan(params.question).steps;
  const reportId = 1;
  const tick = async (ms: number) => {
    await delay(ms);
    return mockStopRequested;
  };

  for (const [i, step] of steps.entries()) {
    mockEmit({ type: "Notice", message: `Step ${i + 1}/${steps.length}: ${step.question}` });
    for (const query of step.searchQueries ?? [step.question]) {
      mockEmit({ type: "Searching", query });
      if (await tick(250)) break;
    }
    if (mockStopRequested) break;
    mockEmit({ type: "Fetched", url: `https://example.com/${i + 1}`, tier: "http", chars: 4321 });
    mockEmit({ type: "FactExtracted", sub_q: i, source_id: i + 1, claim: `Mock evidence for “${step.question.slice(0, 48)}”` });
    await tick(150);
  }

  mockEmit({ type: "Verified", supported: steps.length, unsupported: 0, stage1_passes: steps.length, stage2_supported: steps.length, stage2_contradicted: 0, stage2_unsupported: 0, stage2_failed: 0 });
  for (const step of steps) {
    mockEmit({ type: "SectionWritten", title: step.question, markdown: MOCK_MARKDOWN, confidence: 0.75, citation_count: 2 });
    await tick(100);
  }
  if (mockStopRequested) {
    mockEmit({ type: "Notice", message: "Research cancelled by user; writing partial report" });
  }
  mockEmit({ type: "Notice", message: "Research transcript saved to chats/research-001.json" });
  mockEmit({ type: "Done", report_id: reportId, report_path: "outputs/deep-research/research-mock-001.md" });
  return reportId;
}

export async function planDeepResearch(params: PlanDeepResearchParams): Promise<DeepResearchPlan> {
  if (!isTauri) {
    await delay(400);
    // Match the real command's behaviour: offline mode coerces internet steps to local.
    const plan = mockPlan(params.question);
    if ((params.mode ?? "online") === "offline") {
      plan.steps = plan.steps.map((step) => ({ ...step, source: "local" as const }));
    }
    return plan;
  }
  return invoke<DeepResearchPlan>("plan_deep_research", {
    question: params.question,
    mode: params.mode ?? "online",
    project_id: params.projectId,
  });
}

export async function runDeepResearch(params: RunDeepResearchParams): Promise<number> {
  if (!isTauri) {
    return mockRun(params);
  }
  return invoke<number>("run_deep_research", {
    question: params.question,
    mode: params.mode ?? "online",
    project_id: params.projectId,
    provided_urls: params.providedUrls ?? [],
    use_browser: params.useBrowser ?? true,
    budget_preset: params.budgetPreset ?? "standard",
    plan: params.plan ?? null,
  });
}

export async function stopDeepResearch(projectId: string): Promise<void> {
  if (!isTauri) {
    mockStopRequested = true;
    return;
  }
  return invoke<void>("research_stop", {
    project_id: projectId,
  });
}

/** Re-render a finished run's report in the requested format. Returns the output path. */
export async function exportResearchReport(
  params: ExportResearchReportParams,
): Promise<string> {
  if (!isTauri) {
    await delay(300);
    const ext = params.format === "markdown" ? "md" : params.format === "text" ? "txt" : params.format === "html" ? "html" : "json";
    return `outputs/deep-research/research-${String(params.reportId).padStart(3, "0")}-export.${ext}`;
  }
  return invoke<string>("export_research_report", {
    project_id: params.projectId,
    report_id: params.reportId,
    format: params.format,
    citation_style: params.citationStyle ?? "numeric",
  });
}

/** Reveal a path in the OS file manager. */
export async function revealPath(path: string): Promise<void> {
  if (!isTauri) {
    console.info("[mock] revealPath:", path);
    return;
  }
  return invoke<void>("reveal_path", { path });
}

export async function onResearchEvent(
  handler: (event: ResearchEvent) => void
): Promise<() => void> {
  if (!isTauri) {
    mockHandlers.add(handler);
    return () => {
      mockHandlers.delete(handler);
    };
  }
  return listen<ResearchEvent>("research:event", handler);
}
