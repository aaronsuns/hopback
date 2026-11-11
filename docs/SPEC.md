# Hopback — functional specification

Hopback is a macOS menu bar app for people who run several Claude Code sessions in VS Code at once. It shows
which session each VS Code window is on, tells you when a session is waiting for you, and gets you back to any
session — open or past — with a hotkey or a search.

This document describes **behaviour**. How to build it is up to the implementer. Hopback is an independent
open-source tool and is not affiliated with Anthropic; say so in the README.

## Platform and stack

- macOS 13 (Ventura) or newer, Apple silicon and Intel (universal build). VS Code with the Claude Code
  extension. Other editors (Cursor, Insiders) are out of scope for v1.
- Tauri 2: Rust backend, TypeScript frontend (React + Vite is fine), Vitest and `cargo test` for tests.
- Menu bar only, no Dock icon. Bundle identifier `se.zexa.hopback`. Product name `Hopback`. MIT licence,
  public repo `github.com/aaronsuns/hopback`, product page `https://hopback.zexa.se` (placeholder for now).
- A second small binary, `hopback-hook`, ships inside the app bundle (`Contents/MacOS/hopback-hook`).

## Session states

| State | Glyph | Meaning |
|---|---|---|
| approve | ⚠️ | waiting for the user to approve a tool call |
| question | ❓ | waiting for an answer to a question, or for a plan to be reviewed |
| working | ⚙️ | busy: a prompt was submitted or a tool is running |
| idle | 💤 | finished its turn, nothing pending |

"Needs you" = approve or question.

## Where the data comes from

### 1. The hook (live state)

The repo is also a Claude Code **plugin marketplace**: `.claude-plugin/marketplace.json` lists one plugin,
`plugins/hopback/`, whose `hooks/hooks.json` runs
`/Applications/Hopback.app/Contents/MacOS/hopback-hook` on the hook events Claude Code offers for this
(permission request, notification, pre/post tool use and its failure, user prompt submit, stop, session start
and end). Read the current Claude Code hooks documentation for the event names and the JSON each one receives
on stdin.

`hopback-hook`:
- reads the event JSON from stdin, derives the new state (permission request → approve; asking-the-user or
  plan-approval tools → question; prompt submit / tool use → working; stop → idle; session end → remove);
- writes one small JSON marker per session to `~/.claude/hopback/<session_id>.json` (state, cwd, transcript
  path, timestamp, the Claude process id if obtainable), atomically (write temp + rename);
- touches `~/.claude/hopback/.hook-seen` so the app can tell the plugin works;
- never prints, never fails: always exit 0, finish within ~50 ms, and do nothing if the input is unexpected;
- is a no-op when the app is not installed (the hooks.json command must tolerate a missing binary).

The app watches that directory (FSEvents / `notify` crate). Markers whose process is gone, or idle markers
older than a day, are treated as ended.

### 2. Transcripts (titles and history)

Claude Code keeps one JSONL transcript per session under `~/.claude/projects/<encoded cwd>/<session id>.jsonl`.
From it derive: the working directory, the title (a custom or summary title entry if present, otherwise the
first real user prompt, trimmed to one line), last-modified time, and the plain text of user and assistant
messages for search. Skip tool results, system reminders and other non-conversation noise. Tolerate partial
last lines and unknown entry types.

### 3. VS Code windows (needs Accessibility)

With the Accessibility permission, read the titles of all VS Code windows (the **Window** menu of the VS Code
app lists every window, including ones on other Spaces) and bring a chosen window to the front by pressing its
menu item. From each window title get the workspace folder name.

Map each window to the Claude session it currently shows. VS Code persists workspace and extension state to
disk (`~/Library/Application Support/Code/User/workspaceStorage/…`) periodically, not instantly; find where the
Claude Code extension's open-session information lives and use it, with a sensible fallback (the most recently
active session whose cwd is that folder). Document what you found in MAINTAINING.md. It is acceptable that the
mapping lags a few seconds behind VS Code.

## Menu bar

Title: `⚠1 ⚙2 · ▢8` = 1 session needs you, 2 working, 8 VS Code windows show a session. `⚠` only appears when
non-zero; `▢` only with Accessibility on.

Menu, top to bottom:
1. Status lines when something is missing: **Accessibility off — Turn on…**, **Needs-you alerts off — Install
   plugin…** (no `.hook-seen` yet), **Update available (x.y.z)**.
2. **Needs you:** every session waiting for the user, shown in a window or not. Header carries ⌃⌥0.
3. **Windows:** one row per VS Code window, numbered by folder name A→Z with its shortcut ⌃⌥1…⌃⌥9.
4. **Working in background:** working sessions that no window shows.
5. **Search open sessions…** ⌃⌘C, **Search history…** ⌃⇧⌘C, **Settings…**, **Diagnostics…**,
   **About Hopback**, **Quit Hopback**.

Each row: state glyph, folder, session title, and how long ago it changed ("now", "3m ago", "2d ago").
Clicking a row goes to the session: raise its window if one shows it; otherwise open the folder in VS Code
and then the conversation (research the Claude Code extension's URI handler or command to open a given
session; fall back to just opening the folder). Idle sessions that no window shows appear only in searches.

## Shortcuts

| Default | Does |
|---|---|
| ⌃⌥1 … ⌃⌥9 | go to VS Code window 1–9 as numbered in the menu |
| ⌃⌥0 | go to the next session that needs you; press again for the one after |
| ⌃⌥⇥ / ⌃⌥⇧⇥ | next / previous window, starting from the frontmost |
| ⌃⌘C | search open sessions |
| ⌃⇧⌘C | search history |

Settings → **Shortcuts** has a recorder per action: click, press keys, **Clear** to remove. While recording,
all Hopback shortcuts are paused. Reject combinations without ⌃ ⌥ or ⌘, duplicates, and ones macOS reserves
(⌘Space, ⌘⇥, ⇧⌘3/4/5, ⌘Q, …), with a short reason.

## Search window

One floating window, two modes: **open sessions** and **history** (every transcript). **⇥** switches mode.
- Type to filter by title and folder. In history, 3+ characters also search conversation text; those matches
  come after title matches and show a short snippet around the hit.
- ↑/↓ select, **⏎** goes to the session, **Esc** closes.
- The history index builds in the background on first run and refreshes when history opens and every
  10 minutes; incremental (only changed transcripts). Show "Building the history index…" while it builds.
  It lives under `~/Library/Caches/se.zexa.hopback/`.

### Ask

Typing `?` followed by a description (or choosing the last row, **Ask Claude**, in history mode) asks Claude to
find the session. Run `claude -p` non-interactively with the working directory set to the history cache, the
allowed tools limited to Read, Grep and Glob, a prompt that explains the cache layout and asks for a JSON list
of session ids with a one-line reason each, and a timeout of ~90 s. **Esc**/**Cancel** kills the process. The
results replace the list, each with its reason. Find `claude` via the setting, else the login shell's PATH,
else `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`.

## Banners

When a session enters approve or question, show a macOS notification (folder + title + what it waits for).
Clicking it goes to the session. One banner per transition, not per poll. Settings can turn off approve and
question banners separately.

## Settings

- **Shortcuts:** as above.
- **General:** banners for approvals / for questions, path to `claude` (empty = auto), start at login
  (default on), check for updates / install automatically, version.

## First run (welcome window)

A checklist that ticks itself off live: **Accessibility** (button opens the right System Settings pane),
**Claude Code plugin** (shows the two plugin install commands with copy buttons; ticks when `.hook-seen` appears),
**Notifications**, **Start at login**. **Finish** closes it; the history index may still be building.

Plugin install commands to show (terminal commands: the VS Code Claude Code panel has no `/plugin`):
```
claude plugin marketplace add aaronsuns/hopback
claude plugin install hopback@hopback
```

## Diagnostics window

One line per check with ✓/✗ and a hint: app version, Accessibility, VS Code running and windows found, plugin
installed (`~/.claude/plugins/installed_plugins.json`) and hook seen, marker directory, transcripts found,
history index status, `claude` found for Ask, notifications allowed, update check. **Copy report** puts it on
the clipboard. It never contains conversation text.

## Updates

Tauri updater plugin with a signed `latest.json` on the newest GitHub release of the public repo. Check on
start and every 6 hours; with **Install automatically** on (default), install in the background and restart
only when no Hopback window is open, then a banner **Hopback updated to x.y.z**. Otherwise the menu shows
**Update available (x.y.z)**. Sign every release with the same code-signing identity so macOS keeps the
Accessibility grant across updates (a self-signed identity is fine until there is an Apple Developer ID).

## Persistence and logging

- Settings JSON under `~/Library/Application Support/se.zexa.hopback/`, atomic writes, corrupt file → defaults.
- Log to `~/Library/Logs/Hopback/hopback.log`, rotated at 1 MB, keep 3. Never conversation text.

## Quality bar

- Transcript parsing, title extraction, hook state derivation, window-title parsing, window numbering, search
  ranking/snippets and shortcut validation are pure functions with unit tests, using made-up fixtures only
  (folders like `web-app`, `api-server`, `infra`).
- The hook binary has its own tests: every event type, malformed input, missing fields, always exit 0.
- `scripts/check.sh` runs format, lint, typecheck and all tests; CI runs it on every push.
- README for end users: what it does, screenshot, install (DMG + `xattr`, because it is not notarized),
  plugin install, use, shortcuts, search and Ask, updates, FAQ, uninstall. MAINTAINING.md for building and
  releasing.
