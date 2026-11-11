import { invoke } from "@tauri-apps/api/core";

export type SessionState = "approve" | "question" | "working" | "idle";

export interface Row {
  session_id: string;
  folder: string;
  title: string;
  state: SessionState | null;
  glyph: string;
  ago: string;
  window: number | null;
  snippet: string | null;
  content_match: boolean;
  reason: string | null;
}

export interface IndexStatus {
  building: boolean;
  sessions: number;
  last_refresh_ms: number | null;
  last_error: string | null;
}

export interface Settings {
  shortcuts: Record<string, string | null>;
  banners_approve: boolean;
  banners_question: boolean;
  claude_path: string;
  start_at_login: boolean;
  check_updates: boolean;
  install_updates_automatically: boolean;
  welcome_done: boolean;
}

export interface ShortcutRow {
  id: string;
  label: string;
  accel: string | null;
  display: string;
}

export interface SettingsView {
  settings: Settings;
  shortcuts: ShortcutRow[];
  version: string;
}

export type PluginStep = "not_installed" | "installed" | "ready";

export interface WelcomeStatus {
  accessibility: boolean;
  plugin: PluginStep;
  start_at_login: boolean;
  plugin_commands: string[];
}

export interface InstallResult {
  ok: boolean;
  claude_found: boolean;
  output: string;
}

export interface Check {
  name: string;
  ok: boolean | null;
  detail: string;
  hint: string;
}

export const api = {
  search: (mode: string, query: string) => invoke<Row[]>("search", { mode, query }),
  indexStatus: () => invoke<IndexStatus>("index_status"),
  refreshHistory: () => invoke<void>("refresh_history"),
  goTo: (sessionId: string) => invoke<void>("go_to", { sessionId }),
  ask: (question: string) => invoke<Row[]>("ask", { question }),
  cancelAsk: () => invoke<void>("cancel_ask"),
  getSettings: () => invoke<SettingsView>("get_settings"),
  setGeneral: (settings: Settings) => invoke<SettingsView>("set_general", { settings }),
  setShortcut: (id: string, accel: string | null) => invoke<SettingsView>("set_shortcut", { id, accel }),
  resetShortcuts: () => invoke<SettingsView>("reset_shortcuts"),
  pauseShortcuts: (paused: boolean) => invoke<void>("pause_shortcuts", { paused }),
  welcomeStatus: () => invoke<WelcomeStatus>("welcome_status"),
  notificationPermission: () => invoke<boolean | null>("notification_permission"),
  installPlugin: () => invoke<InstallResult>("install_plugin"),
  openAccessibility: () => invoke<void>("open_accessibility"),
  testBanner: () => invoke<void>("test_banner"),
  openNotificationSettings: () => invoke<void>("open_notification_settings"),
  setStartAtLogin: (on: boolean) => invoke<boolean>("set_start_at_login", { on }),
  finishWelcome: () => invoke<void>("finish_welcome"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  diagnostics: () => invoke<{ checks: Check[]; report: string }>("get_diagnostics"),
  version: () => invoke<string>("app_version"),
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};
