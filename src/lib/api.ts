// Thin typed layer over the Rust commands. When the page runs in a normal
// browser (`npm run dev`) it falls back to mock data, so the UI can be
// designed without launching Tauri.

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import { listen as tauriListen, type UnlistenFn } from "@tauri-apps/api/event";
import * as mock from "./mock";

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface WinInfo {
  hwnd: number;
  title: string;
  exe: string;
  appId: string;
  minimized: boolean;
  monitor: number;
  /** the app flashed (asks for attention) and hasn't been opened since */
  attention?: boolean;
}

export interface AppEntry {
  name: string;
  target: string;
  iconPath: string;
}

export interface Pinned {
  name: string;
  target: string;
  appId: string;
}

export interface Settings {
  pinned: Pinned[];
  showStats: boolean;
  showMedia: boolean;
  showLabels: boolean;
  centered: boolean;
  allMonitors: boolean;
  autostart: boolean;
  autoHide: boolean;
  floating: boolean;
  /** colour theme id (see theme.ts) */
  theme: string;
  /** "system" (Windows accent), "" (the theme's own) or "#rrggbb" */
  accent: string;
  iconSize: "small" | "medium" | "large";
  roundness: "square" | "medium" | "round";
  clockSeconds: boolean;
  reduceMotion: boolean;
  /** light theme by day: "off" | "windows" (Windows' app mode) | "time" (7-19) */
  autoTheme: "off" | "windows" | "time";
}

export interface Desktops {
  names: string[];
  current: number;
}

export interface SystemStatus {
  battery: { percent: number; charging: boolean } | null;
  online: boolean;
  volume: number;
  muted: boolean;
  cpu: number;
  ram: number;
  accent: string;
}

export interface MediaInfo {
  title: string;
  artist: string;
  playing: boolean;
  source: string;
}

export interface TrayItem {
  id: string;
  tip: string;
  icon: string | null;
}

export interface AudioOut {
  id: string;
  name: string;
  default: boolean;
}

export interface AppVolume {
  pid: number;
  name: string;
  exe: string;
  volume: number;
  muted: boolean;
  active: boolean;
}

export interface WifiNetwork {
  ssid: string;
  bars: number;
  secure: boolean;
  connected: boolean;
}

export interface WifiList {
  networks: WifiNetwork[];
  /** location is off: only saved networks */
  savedOnly: boolean;
}

export interface JumpItem {
  name: string;
  path: string;
  args: string;
  link: boolean;
  task: boolean;
}

export interface WallpaperPalette {
  accent: string;
  a2: string;
  a3: string;
  base: string;
}

export interface BackupInfo {
  id: string;
  /** unix seconds */
  time: number;
  pinned: number;
}

export interface Radios {
  wifi: boolean | null;
  bluetooth: boolean | null;
  network: string | null;
  btDevices: string[];
}

export interface BarInfo {
  monitor: number;
  primary: boolean;
}

export interface Shape {
  x: number;
  y: number;
  w: number;
  h: number;
  r: number;
}

export interface ThumbRect {
  hwnd: number;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface UpdateInfo {
  version: string;
  notes: string;
}

export interface UpdateStatus {
  /** version running now */
  current: string;
  /** newer version on GitHub, once a check found one */
  available: UpdateInfo | null;
}

export interface UpdateProgress {
  done: number;
  total: number | null;
}

export type ShellAction =
  | "start"
  | "search"
  | "taskview"
  | "quick"
  | "notifications"
  | "desktop"
  | "winx"
  | "taskmgr"
  | "settings"
  | "shutdown";

function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (isTauri) return tauriInvoke<T>(cmd, args);
  return mock.invoke<T>(cmd, args);
}

export function listen<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  if (isTauri) return tauriListen<T>(event, (e) => cb(e.payload));
  // design mode: the mock backend dispatches `mock:<event>` on window
  const h = (e: Event) => cb((e as CustomEvent<T>).detail);
  window.addEventListener(`mock:${event}`, h);
  return Promise.resolve(() => window.removeEventListener(`mock:${event}`, h));
}

export const api = {
  getWindows: () => invoke<WinInfo[]>("get_windows"),
  getActive: () => invoke<number>("get_active"),
  windowAction: (hwnd: number, action: "toggle" | "activate" | "minimize" | "close") =>
    invoke<void>("window_action", { hwnd, action }),
  getIcon: (path: string, size = 64) => invoke<string | null>("get_icon", { path, size }),
  getWindowIcon: (hwnd: number, size = 64) => invoke<string | null>("get_window_icon", { hwnd, size }),
  listApps: (refresh = false) => invoke<AppEntry[]>("list_apps", { refresh }),
  resolveShortcut: (path: string) => invoke<string | null>("resolve_shortcut", { path }),
  launch: (target: string, args?: string) => invoke<void>("launch", { target, args }),
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  systemStatus: () => invoke<SystemStatus | null>("system_status"),
  setVolume: (volume?: number, muted?: boolean) => invoke<void>("set_volume", { volume, muted }),
  desktops: () => invoke<Desktops>("desktops"),
  switchDesktop: (index: number | null) => invoke<void>("switch_desktop", { index }),
  mediaArt: () => invoke<string | null>("media_art"),
  wallpaperPalette: () => invoke<WallpaperPalette | null>("wallpaper_palette"),
  listBackups: () => invoke<BackupInfo[]>("list_backups"),
  readBackup: (id: string) => invoke<Settings>("read_backup", { id }),
  exportSettings: () => invoke<boolean>("export_settings"),
  importSettings: () => invoke<Settings | null>("import_settings"),
  jumpList: (hwnd: number | undefined, target: string, exe: string) =>
    invoke<JumpItem[]>("jump_list", { hwnd, target, exe }),
  appVolumes: () => invoke<AppVolume[]>("app_volumes"),
  setAppVolume: (pid: number, volume?: number, muted?: boolean) => invoke<void>("set_app_volume", { pid, volume, muted }),
  wifiNetworks: () => invoke<WifiList>("wifi_networks"),
  wifiConnect: (ssid: string, password?: string) => invoke<void>("wifi_connect", { ssid, password }),
  wifiDisconnect: () => invoke<void>("wifi_disconnect"),
  audioOutputs: () => invoke<AudioOut[]>("audio_outputs"),
  setAudioOutput: (id: string) => invoke<void>("set_audio_output", { id }),
  mediaInfo: () => invoke<MediaInfo | null>("media_info"),
  mediaAction: (action: "toggle" | "next" | "prev") => invoke<void>("media_action", { action }),
  shell: (action: ShellAction) => invoke<void>("shell_action", { action }),
  showFlyout: (panel: string, x: number, width: number, height: number, data?: unknown) =>
    invoke<void>("show_flyout", { panel, x, width, height, data }),
  /** hide the flyout window now (the flyout page, after its fade-out) */
  hideFlyout: () => invoke<void>("hide_flyout"),
  /** close the flyout nicely: its panel fades out, then the window hides */
  closeFlyout: () => invoke<void>("request_close_flyout"),
  fitFlyout: (height: number) => invoke<void>("fit_flyout", { height }),
  systemAccent: () => invoke<string>("system_accent"),
  barInfo: () => invoke<BarInfo | null>("bar_info"),
  setRegion: (shapes: Shape[]) => invoke<void>("set_region", { shapes }),
  showPreview: (x: number, width: number, height: number, data: unknown) =>
    invoke<void>("show_preview", { x, width, height, data }),
  setThumbs: (rects: ThumbRect[]) => invoke<void>("set_thumbs", { rects }),
  hidePreview: (delay = 0) => invoke<void>("hide_preview", { delay }),
  previewHold: (on: boolean) => invoke<void>("preview_hold", { on }),
  trayIcons: () => invoke<TrayItem[]>("tray_icons"),
  trayClick: (id: string, kind: "left" | "right" | "double", x: number, y: number) =>
    invoke<void>("tray_click", { id, kind, x, y }),
  radioStatus: () => invoke<Radios | null>("radio_status"),
  setRadio: (kind: "wifi" | "bluetooth", on: boolean) => invoke<void>("set_radio", { kind, on }),
  quit: () => invoke<void>("quit_app"),
  updateStatus: () => invoke<UpdateStatus>("update_status"),
  checkUpdate: () => invoke<UpdateStatus>("check_update"),
  /** downloads the update and restarts into it (only returns on failure) */
  installUpdate: () => invoke<void>("install_update"),
};

// ---------------------------------------------------------------- icons --

const iconCache = new Map<string, Promise<string | null>>();

/**
 * Icon for an exe / lnk / shell: path (falling back to the window's own
 * icon), extracted at the exact device-pixel size so it stays sharp on
 * every monitor scale. Cached for the whole session.
 */
export function iconFor(path: string, hwnd?: number, cssSize = 32): Promise<string | null> {
  const px = Math.round(cssSize * (window.devicePixelRatio || 1));
  const key = `${path.toLowerCase() || `hwnd:${hwnd}`}@${px}`;
  let p = iconCache.get(key);
  if (!p) {
    p = (path ? api.getIcon(path, px) : Promise.resolve(null)).then((url) =>
      url ?? (hwnd ? api.getWindowIcon(hwnd, px) : null),
    );
    iconCache.set(key, p);
  }
  return p;
}

/** "C:\\...\\Code.exe" -> "Code" */
export function prettyExe(path: string): string {
  const file = path.split(/[\\/]/).pop() ?? path;
  const base = file.replace(/\.exe$/i, "");
  return base.charAt(0).toUpperCase() + base.slice(1);
}
