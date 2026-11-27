// Colour themes. Every window (bar, panels, previews) applies the same theme
// by setting a few CSS variables on <html>; all the styles read those.

import { api, listen, type Settings, type WallpaperPalette } from "./api";

export interface Theme {
  id: string;
  name: string;
  scheme: "dark" | "light";
  /** default accent + the two "aurora" glow colours */
  accent: string;
  a2: string;
  a3: string;
  /** islands (top -> bottom) */
  surface: [string, string];
  /** fixed bar strip */
  strip: [string, string];
  /** panels / previews base */
  glass: [string, string];
  /** start button tile */
  tile: [string, string];
}

export const THEMES: Theme[] = [
  {
    id: "aurora",
    name: "Aurora",
    scheme: "dark",
    accent: "#4cc2ff",
    a2: "#8b5cff",
    a3: "#ff5fc8",
    surface: ["#1a1b28", "#0d0e16"],
    strip: ["#161724", "#0d0e16"],
    glass: ["#1a1b28", "#0c0d15"],
    tile: ["#24263f", "#121322"],
  },
  {
    id: "grafite",
    name: "Grafite",
    scheme: "dark",
    accent: "#8ab4f8",
    a2: "#9aa0a6",
    a3: "#c9cdd3",
    surface: ["#232427", "#141517"],
    strip: ["#1d1e21", "#121315"],
    glass: ["#212226", "#121315"],
    tile: ["#2c2d31", "#18191c"],
  },
  {
    id: "oceano",
    name: "Oceano",
    scheme: "dark",
    accent: "#2dd4bf",
    a2: "#38bdf8",
    a3: "#6366f1",
    surface: ["#10232f", "#08141c"],
    strip: ["#0e1e29", "#07111a"],
    glass: ["#0f2230", "#07121a"],
    tile: ["#163447", "#0a1a25"],
  },
  {
    id: "tramonto",
    name: "Tramonto",
    scheme: "dark",
    accent: "#ff9e57",
    a2: "#ff5f8f",
    a3: "#ffd166",
    surface: ["#2a1720", "#150b11"],
    strip: ["#24131b", "#13090e"],
    glass: ["#291620", "#13090e"],
    tile: ["#3b1f2c", "#1c0e15"],
  },
  {
    id: "foresta",
    name: "Foresta",
    scheme: "dark",
    accent: "#4ade80",
    a2: "#a3e635",
    a3: "#2dd4bf",
    surface: ["#16251c", "#0b140f"],
    strip: ["#132118", "#0a120d"],
    glass: ["#15241b", "#0a130e"],
    tile: ["#1f3628", "#0f1c14"],
  },
  {
    id: "ametista",
    name: "Ametista",
    scheme: "dark",
    accent: "#c084fc",
    a2: "#f472b6",
    a3: "#818cf8",
    surface: ["#221a33", "#110c1c"],
    strip: ["#1d162c", "#0f0a18"],
    glass: ["#211932", "#100b1a"],
    tile: ["#2f2347", "#170f25"],
  },
  {
    id: "chiaro",
    name: "Chiaro",
    scheme: "light",
    accent: "#0a6cff",
    a2: "#8b5cff",
    a3: "#ff5fc8",
    surface: ["#f9f9fd", "#e9e9f3"],
    strip: ["#f3f3f9", "#e4e4ee"],
    glass: ["#ffffff", "#f0f0f7"],
    tile: ["#ffffff", "#e3e4f0"],
  },
];

/** Accent colours offered next to "Windows" and the theme's own. */
export const ACCENTS = ["#4cc2ff", "#2dd4bf", "#4ade80", "#facc15", "#ff9e57", "#f87171", "#f472b6", "#c084fc"];

// ---- "Dallo sfondo": colours taken from the desktop wallpaper ----

export const WALLPAPER_ID = "sfondo";
let wallpaper: Theme | null = null;

/** "#rrggbb" mixed with "#rrggbb" (t = share of the second). */
function mix(a: string, b: string, t: number): string {
  const p = (h: string) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [x, y] = [p(a), p(b)];
  return "#" + x.map((v, i) => Math.round(v + (y[i] - v) * t).toString(16).padStart(2, "0")).join("");
}

function wallpaperTheme(pal: WallpaperPalette): Theme {
  const b = pal.base;
  return {
    id: WALLPAPER_ID,
    name: "Dallo sfondo",
    scheme: "dark",
    accent: pal.accent,
    a2: pal.a2,
    a3: pal.a3,
    surface: [mix(b, "#ffffff", 0.02), mix(b, "#000000", 0.45)],
    strip: [mix(b, "#000000", 0.12), mix(b, "#000000", 0.5)],
    glass: [b, mix(b, "#000000", 0.5)],
    tile: [mix(b, "#ffffff", 0.08), mix(b, "#000000", 0.3)],
  };
}

/** The wallpaper theme, once its colours are known (for the picker). */
export function currentWallpaperTheme(): Theme | null {
  return wallpaper;
}

async function refreshWallpaper(): Promise<boolean> {
  const pal = await api.wallpaperPalette().catch(() => null);
  if (!pal) return false;
  const next = wallpaperTheme(pal);
  const changed = JSON.stringify(next) !== JSON.stringify(wallpaper);
  wallpaper = next;
  return changed;
}

export function themeById(id: string | undefined): Theme {
  if (id === WALLPAPER_ID) return wallpaper ?? { ...THEMES[0], id: WALLPAPER_ID, name: "Dallo sfondo" };
  return THEMES.find((t) => t.id === id) ?? THEMES[0];
}

let systemAccent: string | null = null;
let current: Partial<Settings> | null = null;

// ---- automatic light theme by day ----

const LIGHT_FROM = 7; // 7:00
const LIGHT_TO = 19; // 19:00

/** true = light now, false = dark now, null = automatic switching off */
function autoLight(s: Partial<Settings> | null): boolean | null {
  switch (s?.autoTheme) {
    case "windows":
      // WebView2 reports Windows' "app mode" (light / dark) here
      return window.matchMedia("(prefers-color-scheme: light)").matches;
    case "time": {
      const h = new Date().getHours();
      return h >= LIGHT_FROM && h < LIGHT_TO;
    }
    default:
      return null;
  }
}

/** Theme to show right now: by day "Chiaro", by night the user's own. */
export function effectiveThemeId(s: Partial<Settings> | null): string | undefined {
  const light = autoLight(s);
  if (light === null) return s?.theme;
  if (light) return "chiaro";
  return s?.theme === "chiaro" ? "aurora" : s?.theme; // chosen theme, but dark

}

/** Colour actually used for the accent with these settings. */
export function resolveAccent(s: Partial<Settings> | null, theme = themeById(effectiveThemeId(s))): string {
  const a = s?.accent ?? "system";
  if (a === "system") return systemAccent ?? theme.accent;
  if (/^#[0-9a-f]{6}$/i.test(a)) return a;
  return theme.accent;
}

/** Writes the theme's variables on <html>. Cheap: a dozen style properties. */
export function applyTheme(s: Partial<Settings> | null) {
  current = s;
  const t = themeById(effectiveThemeId(s));
  const root = document.documentElement;
  const set = (k: string, v: string) => root.style.setProperty(k, v);
  set("--accent", resolveAccent(s, t));
  set("--aurora-2", t.a2);
  set("--aurora-3", t.a3);
  set("--surface-1", t.surface[0]);
  set("--surface-2", t.surface[1]);
  set("--strip-1", t.strip[0]);
  set("--strip-2", t.strip[1]);
  set("--glass-1", t.glass[0]);
  set("--glass-2", t.glass[1]);
  set("--tile-1", t.tile[0]);
  set("--tile-2", t.tile[1]);
  root.dataset.scheme = t.scheme;
  root.dataset.theme = t.id;
  // "Riduci animazioni"
  root.classList.toggle("reduce-motion", !!s?.reduceMotion);
}

/** The Windows accent changed (the bar polls it): re-apply if in use. */
export function setSystemAccent(color: string | undefined | null) {
  if (!color || color === systemAccent) return;
  systemAccent = color;
  if ((current?.accent ?? "system") === "system") applyTheme(current);
}

/**
 * Applies the saved theme now and follows every change. Call once per window
 * (in onMount); returns the cleanup function.
 */
export function followTheme(): () => void {
  const apply = async (s: Settings | null) => {
    applyTheme(s);
    // wallpaper theme: colours come from the backend (first time / changed)
    if (s?.theme === WALLPAPER_ID && (await refreshWallpaper())) applyTheme(s);
  };
  api.getSettings().then(apply);
  api.systemAccent().then(setSystemAccent).catch(() => {});
  const un = listen<Settings>("settings-changed", apply);
  // the wallpaper may change (slideshow, Spotlight): check now and then
  const t = setInterval(async () => {
    if (current?.theme === WALLPAPER_ID && (await refreshWallpaper())) applyTheme(current);
  }, 5 * 60_000);
  // day / night: Windows switching light <-> dark...
  const mq = window.matchMedia("(prefers-color-scheme: light)");
  const onScheme = () => current?.autoTheme === "windows" && applyTheme(current);
  mq.addEventListener("change", onScheme);
  // ...or the clock passing 7:00 / 19:00 (checked once a minute)
  let lastLight = autoLight(current);
  const t2 = setInterval(() => {
    const l = autoLight(current);
    if (l !== lastLight) {
      lastLight = l;
      applyTheme(current);
    }
  }, 60_000);
  return () => {
    clearInterval(t);
    clearInterval(t2);
    mq.removeEventListener("change", onScheme);
    un.then((f) => f());
  };
}

/** For the picker: makes sure the wallpaper colours are loaded. */
export async function loadWallpaperTheme(): Promise<Theme | null> {
  await refreshWallpaper();
  return wallpaper;
}
