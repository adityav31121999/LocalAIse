import { delay, invoke, isTauri } from "./index";
import { getMockProject, mockActivity } from "./mock";
import type { ActivityEvent } from "../types";

export async function listActivity(projectId: string): Promise<ActivityEvent[]> {
  if (isTauri) return invoke<ActivityEvent[]>("list_activity", { projectId });
  await delay(140);
  return mockActivity(getMockProject(projectId));
}
