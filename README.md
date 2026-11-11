# Hopback

Hopback is a macOS menu bar app for people who run several Claude Code sessions in VS Code at
once. It shows which session each VS Code window is on, tells you when a session is waiting for
you, and gets you back to any session, open or past, with a hotkey or a search.

> Hopback is an independent open-source project. It is not affiliated with, endorsed by or
> supported by Anthropic. "Claude" and "Claude Code" are Anthropic's names for their products.

<!-- TODO: add docs/screenshot.png (the open menu) with the first release and show it here. -->
*Screenshot coming with the first release.*

## What it does

The menu bar shows a short summary, for example `⚠1 ⚙2 · ▢8`: one session needs you, two are
working, and eight VS Code windows show a Claude Code session.

| Glyph | Meaning |
|---|---|
| ⚠️ | waiting for you to approve a tool call |
| ❓ | waiting for an answer to a question, or for a plan to be reviewed |
| ⚙️ | working |
| 💤 | finished its turn, nothing pending |

Open the menu to see:

- **Needs you** — every session waiting for you, whether a window shows it or not.
- **Windows** — every VS Code window, numbered A→Z by folder, with the session it shows.
- **Working in background** — sessions that are busy but not shown in any window.

Click a row to go there. If a window shows the session, Hopback brings that window to the
front, even from another Space. Otherwise it opens the folder in VS Code and then the
conversation.

When a session starts waiting for you, Hopback shows a banner. Click it to go to the session.

## Requirements

- macOS 13 (Ventura) or newer, Apple silicon or Intel.
- Visual Studio Code with the Claude Code extension. Cursor and VS Code Insiders are not
  supported yet.

## Install

1. Download `Hopback.dmg` from the [latest release](https://github.com/aaronsuns/hopback/releases/latest)
   and drag Hopback to Applications.
2. Hopback is not notarized by Apple yet, so macOS will refuse to open it the first time. Clear the
   quarantine flag once in Terminal:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Hopback.app
   ```

3. Open Hopback. A welcome window walks you through the rest:
   - **Accessibility** — lets Hopback list VS Code's windows and raise one. Turn Hopback on in
     System Settings → Privacy & Security → Accessibility.
   - **Claude Code plugin** — see below.
   - **Notifications** — allow banners.
   - **Start at login** — on by default.

### The Claude Code plugin

The plugin is what tells Hopback a session is working or waiting. In Claude Code (in VS Code or a
terminal), run:

```
/plugin marketplace add aaronsuns/hopback
/plugin install hopback@hopback
```

Then start a new session (or restart running ones). The plugin runs a tiny program inside
Hopback.app on Claude Code's hook events. It writes one small status file per session to
`~/.claude/hopback/` and nothing else. If Hopback is not installed, the plugin does nothing.

Without the plugin, Hopback still shows your windows and lets you search, but it cannot tell when
a session needs you.

## Use

### Shortcuts

| Default | Does |
|---|---|
| ⌃⌥1 … ⌃⌥9 | go to VS Code window 1–9, as numbered in the menu |
| ⌃⌥0 | go to the next session that needs you; press again for the one after |
| ⌃⌥⇥ / ⌃⌥⇧⇥ | next / previous VS Code window |
| ⌃⌘C | search open sessions |
| ⌃⇧⌘C | search history |

Change them in **Settings → Shortcuts**: click a shortcut, press the new keys, or **Clear** it.
Hopback refuses combinations without ⌃, ⌥ or ⌘, duplicates, and ones macOS keeps for itself.

### Search

The search window has two modes; **⇥** switches between them.

- **Open sessions** — sessions that are running or shown in a window.
- **History** — every Claude Code conversation on this Mac. Type to filter by title and folder.
  With three or more characters Hopback also searches the conversation text; those matches come
  after title matches, with a snippet around the hit.

**↑/↓** select, **⏎** goes to the session, **Esc** closes.

The first time, Hopback builds a history index in the background ("Building the history
index…"). After that it only re-reads conversations that changed.

### Ask Claude

Can't remember the words? Type `?` and describe the session, for example
`? the one where we fixed the flaky login test`, or pick **Ask Claude** at the end of the history
list. Hopback runs `claude -p` over its history index with read-only tools (Read, Grep, Glob) and
shows the sessions Claude picks, each with a one-line reason. **Esc** or **Cancel** stops it.

Ask uses your own Claude Code login and counts towards your usage like any other prompt. Hopback
looks for `claude` on your login shell's `PATH`, then in `/opt/homebrew/bin`, `/usr/local/bin`
and `~/.local/bin`. You can set the path in **Settings → General**.

## Updates

Hopback checks for updates when it starts and every 6 hours. With **Install automatically** on
(the default) it installs an update in the background and restarts when no Hopback window is open.
Otherwise the menu shows **Update available**. Both are in **Settings → General**.

## Privacy

Hopback runs entirely on your Mac and sends nothing anywhere, except the update check (a request
to GitHub for the latest release) and Ask, which runs your own `claude`. It keeps:

| What | Where |
|---|---|
| Per-session status files (from the plugin) | `~/.claude/hopback/` |
| History index (copies of conversation text, for search) | `~/Library/Caches/se.zexa.hopback/` |
| Settings | `~/Library/Application Support/se.zexa.hopback/` |
| Log (never conversation text) | `~/Library/Logs/Hopback/` |

## FAQ

**The menu shows "Accessibility off".** Open System Settings → Privacy & Security → Accessibility
and turn Hopback on. If it is already on but Hopback still says off, remove it with **−** and add
it again (this can happen after an update with a different signature).

**A window shows the wrong session, or none.** VS Code saves which conversation a window shows
every so often, not instantly, so Hopback can lag a few seconds behind. Until then it guesses the
most recently active session in that folder.

**Sessions never show ⚠️ or ❓.** Install the plugin (see above), then restart your sessions.
**Diagnostics…** in the menu shows whether the plugin is installed and has run.

**Does it work with Claude Code in a terminal?** Partly: sessions show up in Needs you, Working in
background and search, but only VS Code windows are listed and raised.

**Something's off.** Open **Diagnostics…** from the menu and use **Copy report** when you open an
issue. The report contains no conversation text.

## Uninstall

1. In Claude Code: `/plugin uninstall hopback@hopback` and `/plugin marketplace remove hopback`.
2. Quit Hopback and move `/Applications/Hopback.app` to the Bin.
3. Optionally remove its data:

   ```sh
   rm -rf ~/.claude/hopback ~/Library/Caches/se.zexa.hopback \
     "$HOME/Library/Application Support/se.zexa.hopback" ~/Library/Logs/Hopback
   ```

4. Remove Hopback from System Settings → Privacy & Security → Accessibility, and from General →
   Login Items if it is still listed.

## Licence

MIT — see [LICENSE](LICENSE). Website: <https://hopback.zexa.se>.
