import { delay, invoke, isTauri } from "./index";

export interface AnalysisRequest {
  projectId: string;
  action: "summarize" | "extract_tables" | "flag_inconsistencies" | "compare";
  fileName: string;
  compareWith?: string;
  saveAs?: string;
}

export interface AnalysisResult {
  content: string;
  savedToFile?: string;
}

export async function analyzeDocument(request: AnalysisRequest): Promise<AnalysisResult> {
  if (isTauri) {
    return invoke<AnalysisResult>("analyze_document", { request });
  }

  await delay(600);
  return {
    content: `[Simulation] Analysis for ${request.fileName} (${request.action}):\n\n` +
      `• Key finding 1: Document parsed and processed cleanly.\n` +
      `• Key finding 2: Verified consistency against project guidelines.\n` +
      `• Summary: Map-reduce summarization completed successfully.`,
    savedToFile: request.saveAs ? `outputs/${request.saveAs}` : undefined,
  };
}
