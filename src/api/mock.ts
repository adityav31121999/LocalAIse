/** Browser-mode fixtures. Used only when Tauri is not available. */

import type {
  ActivityEvent,
  ChatSession,
  ChatStreamResult,
  ChatSummary,
  Citation,
  DirectoryEntry,
  Project,
  SourceFile,
  SourceStatus,
} from "../types";

function minutesAgo(minutes: number): string {
  return new Date(Date.now() - minutes * 60_000).toISOString();
}

function daysAgo(days: number): string {
  return new Date(Date.now() - days * 86_400_000).toISOString();
}

export const MOCK_PROJECTS: Project[] = [
  {
    id: "budget-2026",
    name: "Budget 2026",
    path: "C:\\Users\\you\\Documents\\localAIse\\Budget 2026",
    template: "budget",
    sourceCount: 14,
    internetEnabled: false,
    systemPrompt: null,
    diaryEnabled: false,
    customFolders: [],
    model: "Qwen2.5 7B Instruct",
    lastOpenedAt: minutesAgo(42),
  },
  {
    id: "thesis-research",
    name: "Thesis Research",
    path: "D:\\Research\\Thesis",
    template: "research",
    sourceCount: 38,
    internetEnabled: true,
    systemPrompt: null,
    diaryEnabled: false,
    customFolders: [],
    model: "Llama 3.1 8B Instruct",
    lastOpenedAt: minutesAgo(190),
  },
  {
    id: "website-copy",
    name: "Website Copy",
    path: "C:\\Users\\you\\Documents\\localAIse\\Website Copy",
    template: "content-writing",
    sourceCount: 9,
    internetEnabled: false,
    systemPrompt: null,
    diaryEnabled: false,
    customFolders: [],
    model: "Qwen2.5 7B Instruct",
    lastOpenedAt: daysAgo(1),
  },
  {
    id: "field-notes",
    name: "Field Notes",
    path: "C:\\Users\\you\\Documents\\localAIse\\Field Notes",
    template: "general",
    sourceCount: 21,
    internetEnabled: false,
    systemPrompt: null,
    diaryEnabled: false,
    customFolders: [],
    model: "Qwen2.5 3B Instruct",
    lastOpenedAt: daysAgo(6),
  },
];

interface Seed {
  name: string;
  status: SourceStatus;
  kb: number;
  modifiedDaysAgo: number;
}

const s = (name: string, status: SourceStatus, kb: number, modifiedDaysAgo: number): Seed => ({
  name,
  status,
  kb,
  modifiedDaysAgo,
});

const SOURCE_SEEDS: Record<string, Seed[]> = {
  "budget-2026": [
    s("invoices/2026-01-acme.pdf", "indexed", 214, 12),
    s("invoices/2026-02-cloud.pdf", "indexed", 188, 8),
    s("budget.xlsx", "indexed", 96, 1),
    s("categories.csv", "indexed", 4, 30),
    s("vendor-contracts.docx", "indexed", 312, 21),
    s("notes.md", "indexed", 12, 0),
    s("outputs/summary-2026-10.md", "pending", 6, 0),
  ],
  "thesis-research": [
    s("papers/federated-learning-survey.pdf", "indexed", 1420, 40),
    s("papers/privacy-budget-dp.pdf", "indexed", 986, 33),
    s("papers/on-device-inference.pdf", "indexed", 1104, 26),
    s("drafts/chapter-2.md", "indexing", 74, 0),
    s("data/participants.csv", "indexed", 22, 14),
    s("bib/references.bib", "indexed", 48, 3),
    s("reading-notes.md", "indexed", 31, 1),
  ],
  "website-copy": [
    s("brand/tone-of-voice.md", "indexed", 9, 18),
    s("brand/messaging.md", "indexed", 14, 18),
    s("drafts/homepage.md", "indexed", 8, 2),
    s("drafts/pricing-page.md", "pending", 7, 0),
  ],
  "field-notes": [
    s("journal/2026-09-site-visit.md", "indexed", 11, 25),
    s("journal/2026-10-interviews.md", "indexed", 18, 4),
    s("photos/README.md", "indexed", 2, 60),
    s("measurements.csv", "indexed", 16, 9),
  ],
};

export function listMockSources(projectId: string): SourceFile[] {
  const seeds = SOURCE_SEEDS[projectId] ?? [];
  return seeds.map((seed) => ({
    id: `${projectId}:${seed.name}`,
    name: seed.name,
    status: seed.status,
    sizeBytes: seed.kb * 1024,
    modifiedAt: daysAgo(seed.modifiedDaysAgo),
  }));
}

export function getMockProject(projectId: string): Project {
  const project = MOCK_PROJECTS.find((item) => item.id === projectId);
  if (!project) throw new Error(`Unknown project: ${projectId}`);
  return project;
}

// ---- Mock project directory (browser mode only) ----------------------------
//
// Browser mode has no disk access, so the Files view works on an in-memory tree
// seeded from the same fixture sources. The shapes and checks mirror the Rust
// commands so the UI behaves the same in both modes.

interface MockNode {
  path: string;
  isDir: boolean;
  sizeBytes: number;
  modifiedAt: string;
}

const mockTrees: Record<string, MockNode[]> = {};

const MOCK_RESERVED = ["chats", "data", "diary", "outputs", ".localaise"];
const MOCK_STATE_DIR = ".LocalAIse";
const MOCK_PROJECT_FILE = "project.json";

function mockParent(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

function mockName(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? path : path.slice(at + 1);
}

function mockJoin(parent: string, name: string): string {
  return parent ? `${parent}/${name}` : name;
}

function seedMockTree(projectId: string): MockNode[] {
  const nodes: MockNode[] = [];
  const dirs = new Set<string>();

  const addDir = (path: string) => {
    if (!path) return;
    const parts = path.split("/");
    for (let index = 1; index <= parts.length; index += 1) {
      const current = parts.slice(0, index).join("/");
      if (dirs.has(current)) continue;
      dirs.add(current);
      nodes.push({ path: current, isDir: true, sizeBytes: 0, modifiedAt: daysAgo(2) });
    }
  };

  ["chats", "data", "diary", "outputs"].forEach(addDir);

  for (const file of listMockSources(projectId)) {
    addDir(mockParent(file.name));
    nodes.push({
      path: file.name,
      isDir: false,
      sizeBytes: file.sizeBytes,
      modifiedAt: file.modifiedAt,
    });
  }

  for (const [name, size] of [
    ["project.json", 512],
    ["README.md", 1024],
  ] as const) {
    if (!nodes.some((node) => node.path === name)) {
      nodes.push({ path: name, isDir: false, sizeBytes: size, modifiedAt: daysAgo(12) });
    }
  }

  return nodes;
}

function mockTree(projectId: string): MockNode[] {
  if (!mockTrees[projectId]) mockTrees[projectId] = seedMockTree(projectId);
  return mockTrees[projectId];
}

function mockValidateName(name: string): string {
  const cleaned = name.trim();
  if (!cleaned) throw new Error("type a name first.");
  if (cleaned === "." || cleaned === "..") throw new Error("“.” and “..” are not valid names.");
  if (/[/\\:*?"<>|]/.test(cleaned)) {
    throw new Error('a name cannot contain / \\ : * ? " < > or |.');
  }
  if (cleaned.length > 255) throw new Error("that name is too long.");
  return cleaned;
}

function mockEnsureFolder(nodes: MockNode[], path: string): void {
  if (path && !nodes.some((node) => node.path === path && node.isDir)) {
    throw new Error("that location is not a folder.");
  }
}

function mockProtected(path: string): boolean {
  const first = path.split("/")[0];
  return (
    path === "" ||
    first.toLowerCase() === MOCK_STATE_DIR.toLowerCase() ||
    path.toLowerCase() === MOCK_PROJECT_FILE.toLowerCase()
  );
}

function mockReservedAtRoot(name: string): boolean {
  return MOCK_RESERVED.includes(name.toLowerCase());
}

function mockEnsureFree(nodes: MockNode[], path: string): void {
  if (nodes.some((node) => node.path.toLowerCase() === path.toLowerCase())) {
    throw new Error(`“${mockName(path)}” already exists.`);
  }
}

function mockUniqueCopyName(nodes: MockNode[], source: string): string {
  const name = mockName(source);
  const dot = name.lastIndexOf(".");
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const extension = dot > 0 ? name.slice(dot) : "";
  let index = 1;
  for (;;) {
    const suffix = index === 1 ? " copy" : ` copy ${index}`;
    const candidate = `${stem}${suffix}${extension}`;
    if (!nodes.some((node) => node.path === mockJoin(mockParent(source), candidate))) {
      return candidate;
    }
    index += 1;
  }
}

export function mockListDirectory(projectId: string, path: string): DirectoryEntry[] {
  const nodes = mockTree(projectId);
  mockEnsureFolder(nodes, path);
  return nodes
    .filter((node) => mockParent(node.path) === path)
    .sort((a, b) =>
      a.isDir === b.isDir
        ? a.path.toLowerCase().localeCompare(b.path.toLowerCase())
        : a.isDir
          ? -1
          : 1,
    )
    .map((node) => ({
      name: mockName(node.path),
      path: node.path,
      isDir: node.isDir,
      sizeBytes: node.sizeBytes,
      modifiedAt: node.modifiedAt,
    }));
}

export function mockCreateEntry(
  projectId: string,
  parent: string,
  name: string,
  isDir: boolean,
): void {
  const nodes = mockTree(projectId);
  mockEnsureFolder(nodes, parent);
  const cleaned = mockValidateName(name);
  if (!parent && mockReservedAtRoot(cleaned)) {
    throw new Error(`“${cleaned}” is reserved for the app; pick another name.`);
  }
  const path = mockJoin(parent, cleaned);
  mockEnsureFree(nodes, path);
  nodes.push({
    path,
    isDir,
    sizeBytes: 0,
    modifiedAt: new Date().toISOString(),
  });
}

export function mockRenameEntry(projectId: string, path: string, newName: string): void {
  const nodes = mockTree(projectId);
  if (mockProtected(path)) throw new Error("that item is managed by the app and cannot be changed here.");
  const source = nodes.find((node) => node.path === path);
  if (!source) throw new Error("that item no longer exists.");

  const cleaned = mockValidateName(newName);
  const parent = mockParent(path);
  if (!parent && mockReservedAtRoot(cleaned)) {
    throw new Error(`“${cleaned}” is reserved for the app; pick another name.`);
  }

  const target = mockJoin(parent, cleaned);
  if (target !== path) mockEnsureFree(nodes, target);

  for (const node of nodes) {
    if (node.path === path) node.path = target;
    else if (node.path.startsWith(`${path}/`)) {
      node.path = `${target}${node.path.slice(path.length)}`;
    }
  }
  source.modifiedAt = new Date().toISOString();
}

export function mockCopyEntry(projectId: string, path: string): void {
  const nodes = mockTree(projectId);
  if (mockProtected(path)) throw new Error("that item is managed by the app and cannot be changed here.");
  const source = nodes.find((node) => node.path === path);
  if (!source) throw new Error("that item no longer exists.");

  const target = mockJoin(mockParent(path), mockUniqueCopyName(nodes, path));
  const clones = nodes
    .filter((node) => node.path === path || node.path.startsWith(`${path}/`))
    .map((node) => ({
      ...node,
      path: `${target}${node.path.slice(path.length)}`,
      modifiedAt: new Date().toISOString(),
    }));
  nodes.push(...clones);
}

export function mockDeleteEntry(projectId: string, path: string): void {
  const nodes = mockTree(projectId);
  if (mockProtected(path)) throw new Error("that item is managed by the app and cannot be changed here.");
  if (!nodes.some((node) => node.path === path)) throw new Error("that item no longer exists.");

  const remaining = nodes.filter(
    (node) => node.path !== path && !node.path.startsWith(`${path}/`),
  );
  nodes.length = 0;
  nodes.push(...remaining);
}

/** Builds a source-grounded answer so the chat panel has something honest to show. */
export function buildMockAnswer(project: Project, prompt: string): ChatStreamResult {
  const sources = listMockSources(project.id).slice(0, 3);
  const citations: Citation[] = sources.map((file, index) => ({
    index: index + 1,
    fileName: file.name,
    page: file.name.endsWith(".pdf") ? 3 + index : undefined,
  }));

  const lines = sources
    .map((file, index) => `${index + 1}. ${file.name} — this is where the passage for "${prompt.trim()}" sits [${index + 1}].`)
    .join("\n");

  const privacyNote = project.internetEnabled
    ? "Internet access is on for this project, but this answer needed only the local index."
    : `Internet access is off for ${project.name}, so this answer came only from files inside the project folder.`;

  const content = [
    `Scanning ${project.sourceCount} indexed sources in ${project.name}, the strongest support for your question is here:`,
    "",
    lines,
    "",
    `Where this is thin: only ${sources.length} of ${project.sourceCount} sources were close enough to cite. Open the sources panel to index the rest.`,
    "",
    privacyNote,
  ].join("\n");

  const reasoning = [
    `The question is "${prompt.trim()}".`,
    `I should check which files in ${project.name} match it before answering.`,
    `${sources.length} source${sources.length === 1 ? "" : "s"} look relevant, so I will cite ${sources.map((file) => file.name).join(", ") || "nothing"}.`,
    "Now I can write the answer from those passages.",
  ].join("\n");

  return { sessionId: "", title: "", content, reasoning, citations };
}

// ---- Mock chat sessions (browser mode only) --------------------------------

const mockChats: Record<string, ChatSession[]> = {};
const chatCounters: Record<string, number> = {};

function newMockId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 7)}`;
}

function getNextChatNumber(projectId: string): number {
  const current = chatCounters[projectId] ?? 0;
  const next = current + 1;
  chatCounters[projectId] = next;
  return next;
}

export function mockChatList(projectId: string): ChatSummary[] {
  return [...(mockChats[projectId] ?? [])]
    .sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
    .map((session) => ({
      id: session.id,
      title: session.title,
      createdAt: session.createdAt,
      updatedAt: session.updatedAt,
      messageCount: session.messages.length,
    }));
}

export function mockChatRead(projectId: string, chatId: string): ChatSession {
  const session = (mockChats[projectId] ?? []).find((item) => item.id === chatId);
  if (!session) throw new Error("That chat no longer exists.");
  return session;
}

export function mockChatDelete(projectId: string, chatId: string): void {
  mockChats[projectId] = (mockChats[projectId] ?? []).filter((item) => item.id !== chatId);
}

/** Appends a turn to a mock session and returns the resulting session id. */
export function mockChatAppend(
  projectId: string,
  chatId: string | null,
  prompt: string,
  answer: string,
  reasoning: string,
  citations: ChatStreamResult["citations"],
): ChatSummary {
  const now = new Date().toISOString();
  const chats = mockChats[projectId] ?? (mockChats[projectId] = []);
  let session = chatId ? chats.find((item) => item.id === chatId) : undefined;

  if (!session) {
    const chatNumber = getNextChatNumber(projectId);
    session = {
      id: newMockId("chat"),
      title: `chat${chatNumber}`,
      createdAt: now,
      updatedAt: now,
      messages: [],
    };
    chats.push(session);
  }

  session.messages.push(
    { id: newMockId("msg"), role: "user", content: prompt, createdAt: now },
    {
      id: newMockId("msg"),
      role: "assistant",
      content: answer,
      reasoning: reasoning || undefined,
      createdAt: now,
      citations,
    },
  );
  session.updatedAt = now;

  return {
    id: session.id,
    title: session.title,
    createdAt: session.createdAt,
    updatedAt: session.updatedAt,
    messageCount: session.messages.length,
  };
}

export function mockActivity(project: Project): ActivityEvent[] {
  const local: Array<[number, string, string]> = [
    [3, "search_local", "Retrieved 6 passages for the last question"],
    [9, "files_read", "Read budget.xlsx and categories.csv"],
    [26, "index_pipeline", "Re-indexed 2 changed files"],
    [74, "search_local", "Retrieved 4 passages for “compare Q1 and Q2”"],
    [140, "files_read", "Opened invoices/2026-02-cloud.pdf"],
  ];

  const events: ActivityEvent[] = local.map(([minutes, tool, summary], index) => ({
    id: `${project.id}-local-${index}`,
    at: minutesAgo(minutes),
    kind: "local",
    tool,
    summary,
  }));

  if (project.internetEnabled) {
    events.push({
      id: `${project.id}-net-0`,
      at: minutesAgo(52),
      kind: "network",
      tool: "web_fetch",
      summary: "Fetched 1 page requested by the research plan (approved)",
    });
  }

  return events.sort((a, b) => new Date(b.at).getTime() - new Date(a.at).getTime());
}
