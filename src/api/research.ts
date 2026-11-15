import { invoke, listen } from "./index";

export type ResearchEvent =
  | { type: "Planned"; sub_questions: string[] }
  | { type: "Searching"; query: string }
  | { type: "Fetched"; url: string; tier: string; chars: number }
  | { type: "FactExtracted"; sub_q: number; source_id: number; claim: string }
  | { type: "GapFound"; missing: string[] }
  | { type: "SectionWritten"; title: string; markdown: string; confidence?: number; citation_count?: number }
  | { type: "SectionConfidence"; title: string; confidence: number; citation_count: number }
  | {
      type: "Verified";
      supported: number;
      unsupported: number;
      stage1_passes?: number;
      stage2_supported?: number;
      stage2_contradicted?: number;
      stage2_unsupported?: number;
    }
  | { type: "Notice"; message: string }
  | { type: "Done"; report_id: number; report_path?: string }
  | { type: "Error"; message: string };

export interface RunDeepResearchParams {
  question: string;
  mode?: "Online" | "Offline";
  projectId: string;
  providedUrls?: string[];
  useBrowser?: boolean;
  budgetPreset?: "quick" | "standard" | "deep";
}

export async function runDeepResearch(params: RunDeepResearchParams): Promise<number> {
  return invoke<number>("run_deep_research", {
    question: params.question,
    mode: params.mode ?? "Online",
    project_id: params.projectId,
    provided_urls: params.providedUrls ?? [],
    use_browser: params.useBrowser ?? true,
    budget_preset: params.budgetPreset ?? "standard",
  });
}

export async function stopDeepResearch(projectId: string): Promise<void> {
  return invoke<void>("research_stop", {
    project_id: projectId,
  });
}

export async function onResearchEvent(
  handler: (event: ResearchEvent) => void
): Promise<() => void> {
  return listen<ResearchEvent>("research:event", handler);
}
