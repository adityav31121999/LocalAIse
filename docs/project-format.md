# Project format and directory layout

Every project is an ordinary folder. Opening it in a file explorer shows exactly what
localAIse writes — there is no hidden database to export from.

```
projectName/
├── project.json          # manifest: folder layout, diary switch, RAG scope
├── README.md             # written once for new projects
├── data/                 # the user's source files (the knowledge base)
│   ├── notes.md
│   ├── papers/
│   └── ...
├── chats/                # one JSON file per saved chat session
│   ├── chat-001.json
│   └── chat-002.json
├── diary/                # optional: one Markdown page per date
│   ├── 2026-10-22.md
│   └── 2026-10-21.md
├── outputs/              # files the assistant produces for the user
├── <your folders>/       # anything the user creates, tracked in project.json
└── .LocalAIse/           # app-internal state (safe to delete to reset the app)
    ├── index.json        # chunked + embedded source index
    └── activity.log      # JSONL activity log
```

`chats/`, `data/`, `outputs/`, `diary/` and `.LocalAIse/` are reserved names: the user
cannot create folders with those names, and the knowledge base never indexes inside them.
Everything else is free space and is scanned as potential sources.

## `project.json`

The manifest lives at the project root and manages the directory. All fields are
optional for older projects; missing values fall back to the defaults shown here.

```json
{
  "id": "budget-2026-1a2b3c4d",
  "name": "Budget 2026",
  "template": "budget",
  "internetEnabled": false,
  "systemPrompt": null,
  "folders": {
    "chats": "chats",
    "data": "data",
    "diary": "diary",
    "outputs": "outputs",
    "custom": ["invoices", "archive"]
  },
  "diaryEnabled": true,
  "ragPaths": []
}
```

- **`folders`** — the directory layout. Reserved folder names are fixed; `custom` lists
  the folders the user created from the sources panel.
- **`name`** — the label shown in the app. Renaming a project updates this field and
  the app's registry; the folder on disk keeps its path.
- **`diaryEnabled`** — whether this project keeps its own diary. Turning it off keeps
  every page on disk; the folder is simply ignored until it is turned back on.
- **`ragPaths`** — an optional RAG scope: relative folders (`"data"`) or single files
  (`"data/budget.xlsx"`) the knowledge base may read. An empty list means *everything*
  outside the reserved folders, which is the default.

## Chat sessions (`chats/`)

One pretty-printed JSON file per conversation, named with a sequential number:
`chat-001.json`, `chat-002.json`, and so on. The picker above the chat panel shows
the file's number (`Chat 1`, `Chat 2`); the first prompt appears in the bar beside
it. The last 12 messages are replayed to the model as context, and the file is
rewritten after every completed turn. "New chat" starts a new file and tells
llama-server to build the answer from a completely fresh cache (`cache_prompt:
false`).

```json
{
  "id": "chat-001",
  "title": "Where did spending rise the most?",
  "createdAt": "2026-10-22T14:30:05.412Z",
  "updatedAt": "2026-10-22T14:31:42.093Z",
  "messages": [
    {
      "id": "msg-...",
      "role": "user",
      "content": "Where did spending rise the most?",
      "createdAt": "2026-10-22T14:30:05.412Z"
    },
    {
      "id": "msg-...",
      "role": "assistant",
      "content": "Hardware rose the most …",
      "createdAt": "2026-10-22T14:30:41.877Z",
      "citations": [{ "index": 1, "fileName": "data/categories.csv" }]
    }
  ]
}
```

The `title` is still the first prompt (clipped to 60 characters) and is shown next
to the number, so the file name stays stable even when a conversation is about
several things. Chat files written by older versions used timestamp names; they
keep their names and are listed by that saved title instead of a number.

## Diary pages (`diary/`)

When a project opts into its diary, pages are plain Markdown with a small front matter
block. A dated page uses the date as its file name, so one day is always exactly one
page; saving a page under a different date renames the file. Free-form pages (no date)
get a slugged title file name. The personal-space diary uses the same format in
`<personal space>/diary/`.

```markdown
---
id: page-1a2b3c4d
date: 2026-10-22
title: Thursday
createdAt: 2026-10-22T08:12:00.000Z
updatedAt: 2026-10-22T20:40:10.000Z
---

Body text, in Markdown.
```

## Why JSON files, not SQLite

The layout is intentionally plain: chats, sources, diary pages and outputs are all
readable and editable outside the app, and deleting `.LocalAIse/` resets app state
without losing a single file the user wrote. Only the retrieval index
(`.LocalAIse/index.json`) is machine state.

A SQLite index is a possible future optimisation for very large projects; it must remain
a rebuildable cache so the folder keeps the same portable shape.
