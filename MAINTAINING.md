# Maintaining Hopback

The behaviour is specified in [docs/SPEC.md](docs/SPEC.md). This file covers how the code is laid
out, how to build and release, and what we learned about VS Code and Claude Code along the way.

## Layout

| Path | What |
|---|---|
| `crates/hook` | `hopback-hook`, the binary the plugin runs on hook events. Its lib also defines the marker format the app reads. |
| `crates/core` | Pure, unit-tested logic: transcripts, markers, window titles and numbering, session↔window mapping, search, shortcuts, settings, history index, Ask helpers, diagnostics, logging. |
| `src-tauri` | The Tauri app: Accessibility FFI (`ax.rs`), tray menu, banners, global shortcuts, windows, commands for the frontend, updater. |
| `src` | React frontend: one bundle, routed by hash (`#search/open`, `#search/history`, `#settings`, `#welcome`, `#diagnostics`, `#about`). |
| `plugins/hopback`, `.claude-plugin/marketplace.json` | The Claude Code plugin marketplace (one plugin). |
| `scripts/check.sh` | fmt, clippy, tsc, vitest, cargo test. CI runs it. |
| `scripts/build-hook.sh` | Builds `hopback-hook` into `src-tauri/binaries/hopback-hook-<triple>` for Tauri's `externalBin`. |

## Build

Needs Node 20+, Rust stable, and the Xcode Command Line Tools (full Xcode is not needed).

```sh
npm ci
scripts/check.sh                                   # everything CI runs
npx tauri dev                                      # run from source
npx tauri build                                    # host-arch .app and .dmg
```

`tauri.conf.json` runs `npm run build && scripts/build-hook.sh` before bundling, so the hook ends
up in `Hopback.app/Contents/MacOS/hopback-hook`. `src-tauri` will not compile until the sidecar
exists (tauri-build checks `externalBin`); `scripts/check.sh` builds it first.

### Universal build

A universal (arm64 + x86_64) build needs the `x86_64-apple-darwin` standard library, which a
Homebrew `rust` does not ship. Use rustup's toolchain:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
export PATH="$(dirname "$(rustup which cargo)"):$PATH"   # if Homebrew's cargo comes first
npx tauri build --target universal-apple-darwin
```

Output: `target/universal-apple-darwin/release/bundle/macos/Hopback.app` and `…/dmg/`.

### Seeing what the app sees

`Hopback.app/Contents/MacOS/hopback --print-board` prints the current board as JSON (windows,
which session each shows, needs-you and background sessions) and exits. It needs Accessibility
for the terminal it runs in. Handy when the mapping looks wrong.

## Releasing

Releases are built by GitHub Actions (`.github/workflows/release.yml`) when a `v*` tag is pushed:
a universal (arm64 + x86_64) build, with the app and `hopback-hook` signed by the same
self-signed identity, the DMG, the signed updater archive and `latest.json`, all attached to a
published GitHub release.

1. Bump the version everywhere: `package.json`, `Cargo.toml` (`[workspace.package]`, then
   `cargo check` to refresh `Cargo.lock`), `src-tauri/tauri.conf.json`,
   `.claude-plugin/marketplace.json` and `plugins/hopback/.claude-plugin/plugin.json`.
2. Commit and push to `main`; wait for CI to go green.
3. Tag with the release notes as the tag message, and push the tag:

   ```sh
   git tag -a v0.2.0 -m "Hopback 0.2.0" -m "- What changed, one line each"
   git push origin v0.2.0
   ```

   The workflow refuses a tag that does not match `tauri.conf.json`'s version. The tag message
   becomes the release notes and the `notes` in `latest.json`.
4. Watch the run (`gh run watch`). The *Verify signatures* step prints `codesign -dv` for both
   binaries and fails unless both show `Authority=Hopback Self-Signed`.
5. If it fails: fix, then delete the release (if one was created) and the tag, and tag again:

   ```sh
   gh release delete v0.2.0 --yes; git push --delete origin v0.2.0; git tag -d v0.2.0
   ```

Assets on each release: `Hopback.dmg` (so
`https://github.com/aaronsuns/hopback/releases/latest/download/Hopback.dmg` always works),
`Hopback.app.tar.gz` + `.sig` (the updater archive) and `latest.json`.

### Secrets

The workflow uses these GitHub repository secrets. The owner keeps an encrypted backup of each.

| Secret | What |
|---|---|
| `APPLE_CERTIFICATE` | the code-signing certificate and key, base64 `.p12` |
| `APPLE_CERTIFICATE_PASSWORD` | its password |
| `APPLE_SIGNING_IDENTITY` | the certificate's name, `Hopback Self-Signed` |
| `TAURI_SIGNING_PRIVATE_KEY` | the updater (minisign) private key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | its password |

### Code signing

Every release must be signed with the **same** identity, or macOS forgets the Accessibility
grant on update. There is no Apple Developer ID yet, so the identity is a self-signed certificate
and the app is not notarized (hence the `xattr` step in the README). The workflow imports it into
a temporary keychain and marks it trusted for code signing on the runner. Local builds keep
`bundle.macOS.signingIdentity: null` (ad-hoc signing); set `APPLE_SIGNING_IDENTITY` to sign
locally with the certificate in your login keychain.

### Updater

`plugins.updater.pubkey` in `tauri.conf.json` holds the updater public key; the app checks
`https://github.com/aaronsuns/hopback/releases/latest/download/latest.json` (this needs the repo
to be public). `bundle.createUpdaterArtifacts` is `true`, so `tauri build` writes the
`.app.tar.gz` and signs it with `TAURI_SIGNING_PRIVATE_KEY`. Losing that key means existing
installs can no longer update automatically.

### Plugin version

The Claude Code plugin (`plugins/hopback`) ships from this repo, not inside the app. Its version
in `plugin.json` and `marketplace.json` follows the app version so the two are easy to match; the
plugin only runs `/Applications/Hopback.app/Contents/MacOS/hopback-hook`, so an app update changes
the hook without users reinstalling the plugin. Users get plugin changes with
`/plugin marketplace update hopback`.

### Website

`site/` is the landing page at <https://hopback.zexa.se>, a Cloudflare Worker with static assets
(free plan). Deploy with `cd site && npx wrangler@4 deploy`.

## The hook

Claude Code runs `plugins/hopback/hooks/hooks.json`'s command on `SessionStart`,
`UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`, `PostToolUseFailure`,
`Notification`, `Stop` and `SessionEnd`. The command checks that
`/Applications/Hopback.app/Contents/MacOS/hopback-hook` is executable, otherwise drains stdin, and
always exits 0.

State derivation (`crates/hook/src/lib.rs`, `derive`):

| Event | New state |
|---|---|
| `PermissionRequest` | approve; question for `AskUserQuestion` / `ExitPlanMode` |
| `PreToolUse` | working; question for `AskUserQuestion` / `ExitPlanMode` |
| `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit` | working |
| `Notification` `permission_prompt` | approve (kept as question if already question) |
| `Notification` `elicitation_dialog` | question |
| `Stop` | idle |
| `SessionStart` | idle, only if no marker exists |
| `SessionEnd` | marker removed |
| anything else | ignored |

The Claude process id is found by walking up from the hook's parent past the shell that runs the
command (`proc_pidinfo`, no subprocesses). The app treats a marker as ended when that process is
gone, or when it has been idle for over a day. Sessions whose cwd is inside Hopback's own cache
(Ask runs) are ignored.

## Mapping VS Code windows to sessions

What we found (VS Code 1.143, Claude Code extension 2.1.3xx, November 2026):

- **Window list.** The app's **Window** menu lists every window, including ones on other Spaces
  and minimised ones, after the fixed items ending in *Bring All to Front* / *Arrange in Front*.
  Over Accessibility: app element → `AXMenuBar` → item titled `Window` → its `AXMenu` → children;
  separators have an empty `AXTitle`. Pressing an item (`AXPress`) raises that window; setting
  `AXFrontmost` on the app element then activates VS Code. `CGWindowList` cannot see other Spaces,
  which is why the menu is used.
- **Window titles** are `[dirty ●] activeEditor — folder` by default (separator ` — `, em dash).
  With no editor open the title is just the folder. Remote and dev-host windows add `[SSH: …]` or
  `[Extension Development Host]`; multi-root workspaces end in ` (Workspace)`. Custom
  `window.title` settings will confuse the parser.
- **Which workspace storage belongs to a window.** Each
  `~/Library/Application Support/Code/User/workspaceStorage/<hash>/workspace.json` holds the folder
  URI (`{"folder":"file:///…"}`). Hopback matches the window's folder name against the basename of
  that path (the most recently written database wins if two folders share a name).
- **Which session the window shows.** In that directory's `state.vscdb` (SQLite, table
  `ItemTable(key, value)`):
  - `memento/webviewView.claudeVSCodeSidebarSecondary` (the Claude Code sidebar view; other
    `memento/webviewView.claudeVSCode*` keys are read too) holds
    `{"webviewState": "{\"isFullEditor\":false,\"sessionID\":\"<uuid>\",\"sessionUpdatedAt\":<ms>}"}`.
  - Claude Code editor tabs are webview panels with view type `claudeVSCodePanel`; VS Code
    serialises them inside `memento/workbench.parts.editor`, and their state carries the same
    `sessionID` / `sessionUpdatedAt` fields (the extension's `registerWebviewPanelSerializer`
    reads `sessionID` back on restore).
  - The extension's own `workspaceState` (key `Anthropic.claude-code`) has `panelTabSessions`,
    which was empty in every workspace we looked at; it is not used.
  Hopback picks the sidebar or tab session with the newest `sessionUpdatedAt`.
- **Lag.** VS Code flushes `state.vscdb` periodically and on shutdown, not on every change, so the
  mapping can trail what is on screen. Hopback re-reads it every 3 s (the list of workspace
  directories every 60 s), opening the database read-only.
- **Fallback.** If the saved session is unknown (or nothing is saved), Hopback uses the most
  recently active session whose cwd's folder name matches, preferring live ones.
- **Unknown sessions.** If VS Code says a window shows a session the history index has not seen
  yet, Hopback finds `~/.claude/projects/*/<id>.jsonl` and reads its title and cwd directly.

### Opening a session that no window shows

The extension registers a URI handler: `vscode://anthropic.claude-code/open?session=<uuid>`
(also accepts `&prompt=…`). It runs the `claude-vscode.primaryEditor.open` command with that
session id in the **focused** VS Code window. Hopback therefore:

1. runs `open -b com.microsoft.VSCode <cwd>` (focuses the folder's window, or opens one);
2. with Accessibility, waits up to 10 s for that folder's window to be focused (without it,
   1.5 s);
3. opens the URI. If the session id is not a UUID the extension ignores it and you just get the
   folder.

Other extension commands seen in its `package.json` (`claude-vscode.editor.open`,
`claude-vscode.sidebar.open`, …) take no session argument from outside, so the URI handler is the
only way in.

## Transcripts

`~/.claude/projects/<cwd with / and . turned into ->/<session id>.jsonl`, one JSON object per
line. Relevant entry types: `user` / `assistant` (`message.content` is a string or a list of
blocks; only `text` blocks are conversation), `custom-title` (`customTitle`), `summary`
(`summary`), `ai-title` (`aiTitle`). Title priority: custom → summary → AI title → first real
prompt. Skipped as noise: `isMeta`, `isCompactSummary`, `isSidechain`, tool results, and wrappers
such as `<system-reminder>`, `<ide_opened_file>`, `<ide_selection>`, `<command-name>`,
`<local-command-stdout>`, `<task-notification>`. Subagent transcripts live in subdirectories and
are not indexed.

## Spec deviations and notes

- **Notifications allowed** cannot be read with the notification API Hopback uses
  (`mac-notification-sys`, `NSUserNotification`), so Diagnostics shows it as informational and the
  welcome window ticks it after **Send test banner**.
- **Start at login** is only applied when Hopback runs from `/Applications`, so development builds
  never register themselves as login items.
- **⌃⌥0 order**: Needs you is sorted oldest-waiting first; ⌃⌥0 walks that list.
- The search window also accepts `? question` in open-sessions mode.

## TODO

- Real screenshot for the README (`docs/screenshot.png`).
- Banners use the deprecated `NSUserNotification`; move to `UNUserNotificationCenter` (needs a
  signed bundle) and then report the real permission state in Diagnostics.
- Support Cursor / VS Code Insiders (different bundle ids, storage paths and URI schemes).
- Close the search window when it loses focus.
- An end-to-end test that drives a fake `state.vscdb` + markers through `Shared::refresh`.
