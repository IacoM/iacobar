// Fake backend used only when the UI runs in a plain browser (design mode).

const palette = ["#0078d4", "#e3008c", "#107c10", "#ff8c00", "#5c2d91", "#00b7c3", "#d13438", "#498205"];

function letterIcon(name: string): string {
  const color = palette[[...name].reduce((a, c) => a + c.charCodeAt(0), 0) % palette.length];
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${color}"/><stop offset="1" stop-color="${color}" stop-opacity=".7"/></linearGradient></defs><rect width="64" height="64" rx="16" fill="url(#g)"/><text x="32" y="43" font-family="Segoe UI" font-size="30" font-weight="600" text-anchor="middle" fill="#fff">${name[0]}</text></svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}

const windows = [
  { hwnd: 101, title: "Documents - Esplora file", exe: "C:\\Windows\\explorer.exe", minimized: false },
  { hwnd: 102, title: "my-project - Visual Studio Code", exe: "C:\\Apps\\Code.exe", minimized: false },
  { hwnd: 103, title: "Tauri docs - Edge", exe: "C:\\Apps\\msedge.exe", minimized: false },
  { hwnd: 104, title: "GitHub - Edge", exe: "C:\\Apps\\msedge.exe", minimized: true },
  { hwnd: 105, title: "Spotify Premium", exe: "C:\\Apps\\Spotify.exe", minimized: false },
].map((w) => ({
  ...w,
  appId: w.exe.toLowerCase(),
  monitor: 1,
  // design mode: one app asking for attention
  attention: w.hwnd === 105,
}));

// `?apps=N` in design mode: N more running apps, to test a crowded dock
const extra = typeof location !== "undefined" ? Number(new URLSearchParams(location.search).get("apps") ?? 0) : 0;
for (let i = 0; i < extra; i++) {
  const name = String.fromCharCode(65 + (i % 26)) + "pp" + i;
  windows.push({ hwnd: 200 + i, title: `${name} - finestra`, exe: `C:\\Apps\\${name}.exe`, minimized: false, appId: `c:\\apps\\${name.toLowerCase()}.exe`, monitor: 1, attention: false });
}

const apps = [
  "Blocco note", "Calcolatrice", "Code", "Edge", "Esplora file", "Excel", "Firefox", "GIMP", "Git Bash",
  "KeePass", "Outlook", "Paint", "PowerShell", "Spotify", "Teams", "Terminale", "Word", "Impostazioni",
].map((name) => ({ name, target: name, iconPath: name }));

let settings = {
  pinned: [
    { name: "Esplora file", target: "C:\\Windows\\explorer.exe", appId: "c:\\windows\\explorer.exe" },
    { name: "Edge", target: "C:\\Apps\\msedge.exe", appId: "c:\\apps\\msedge.exe" },
    { name: "Outlook", target: "C:\\Apps\\outlook.exe", appId: "c:\\apps\\outlook.exe" },
  ],
  showStats: true,
  showMedia: true,
  showLabels: false,
  centered: true,
  allMonitors: false,
  autostart: true,
  autoHide: false,
  floating: typeof location !== "undefined" && new URLSearchParams(location.search).has("floating"),
  theme: (typeof location !== "undefined" && new URLSearchParams(location.search).get("theme")) || "aurora",
  accent: "",
  iconSize: "medium" as "small" | "medium" | "large",
  roundness: "medium" as "square" | "medium" | "round",
  clockSeconds: false,
  reduceMotion: false,
  autoTheme: "off" as "off" | "windows" | "time",
};
let desktop = 0;

const outputs = [
  { id: "a", name: "Altoparlanti (Realtek(R) Audio)" },
  { id: "b", name: "Cuffie (WH-1000XM5)" },
  { id: "c", name: "DELL U2723QE (Audio HDMI)" },
];
let defaultOut = "a";
const mixer = [
  { pid: 501, name: "Spotify", exe: "C:\\Apps\\Spotify.exe", volume: 0.8, muted: false, active: true },
  { pid: 502, name: "Google Chrome", exe: "C:\\Apps\\Chrome.exe", volume: 0.55, muted: false, active: false },
  { pid: 503, name: "Microsoft Teams", exe: "C:\\Apps\\Teams.exe", volume: 1, muted: true, active: false },
  { pid: 0, name: "Suoni di sistema", exe: "", volume: 1, muted: false, active: false },
];
let wifiOn = "Casa-5G";let volume = 0.42;
let muted = false;
let playing = true;

export async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const r = (v: unknown) => v as T;
  switch (cmd) {
    case "get_windows":
      return r(windows);
    case "tray_icons":
      return r(
        ["KeePass", "OneDrive", "Teams", "Docker", "Discord", "VPN Client"].map((n, i) => ({
          id: n === "VPN Client" ? "orphan:vpnclient.exe" : String(i),
          tip: n,
          icon: letterIcon(n),
        })),
      );
    case "radio_status":
      return r({ wifi: true, bluetooth: true, network: "Casa-5G", btDevices: ["Cuffie WH-1000XM5"] });
    case "desktops":
      return r({ names: ["Lavoro", "Desktop 2", "Svago"], current: desktop });
    case "switch_desktop":
      desktop = typeof args.index === "number" ? args.index : desktop;
      window.dispatchEvent(new CustomEvent("mock:desktops-changed", { detail: { names: ["Lavoro", "Desktop 2", "Svago"], current: desktop } }));
      return r(undefined);
    case "wallpaper_palette":
      return r({ accent: "#79b5f2", a2: "#f27995", a3: "#8b79f2", base: "#282729" });
    case "media_art": {
      const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff7a59"/><stop offset=".5" stop-color="#c2338a"/><stop offset="1" stop-color="#2b1b6b"/></linearGradient></defs><rect width="100" height="100" fill="url(#g)"/><circle cx="68" cy="34" r="16" fill="#ffd27a" opacity=".9"/><path d="M0 78 Q25 60 50 74 T100 70 V100 H0z" fill="#12082e" opacity=".85"/></svg>`;
      return r(`data:image/svg+xml,${encodeURIComponent(svg)}`);
    }
    case "list_backups": {
      const now = Math.floor(Date.now() / 1000);
      return r([
        { id: "config-1", time: now - 3600 * 2, pinned: 7 },
        { id: "config-2", time: now - 86400, pinned: 6 },
        { id: "config-3", time: now - 86400 * 4, pinned: 3 },
      ]);
    }
    case "read_backup":
      return r(structuredClone(settings));
    case "jump_list":
      return r([
        { name: "my-project", path: "C:\\Apps\\Code.exe", args: '--folder-uri "file:///c%3A/Projects/my-project"', link: true, task: false },
        { name: "portale-web-angular", path: "C:\\Apps\\Code.exe", args: '--folder-uri "file:///c%3A/Projects/website"', link: true, task: false },
        { name: "Report Q4.xlsx", path: "D:\\Docs\\Report Q4.xlsx", args: "", link: false, task: false },
        { name: "New Window", path: "C:\\Apps\\Code.exe", args: "-n", link: true, task: true },
      ]);
    case "request_close_flyout":
      window.dispatchEvent(new CustomEvent("mock:flyout-close", { detail: null }));
      return r(undefined);
    case "app_volumes":
      return r(mixer);
    case "set_app_volume": {
      const a = mixer.find((m) => m.pid === args.pid);
      if (a && typeof args.volume === "number") a.volume = args.volume;
      if (a && typeof args.muted === "boolean") a.muted = args.muted;
      return r(undefined);
    }
    case "wifi_networks":
      return r({
        savedOnly: new URLSearchParams(location.search).has("saved"),
        networks: [
          { ssid: "Casa-5G", bars: 4, secure: true, connected: wifiOn === "Casa-5G" },
          { ssid: "Ufficio", bars: 3, secure: true, connected: wifiOn === "Ufficio" },
          { ssid: "Ospiti", bars: 2, secure: false, connected: wifiOn === "Ospiti" },
          { ssid: "Vicino-2.4G", bars: 1, secure: true, connected: false },
        ],
      });
    case "wifi_connect":
      if (args.ssid === "Vicino-2.4G" && !args.password) throw "password";
      wifiOn = String(args.ssid);
      return r(undefined);
    case "wifi_disconnect":
      wifiOn = "";
      return r(undefined);
    case "audio_outputs":
      return r(outputs.map((o) => ({ ...o, default: o.id === defaultOut })));
    case "set_audio_output":
      defaultOut = String(args.id);
      return r(undefined);
    case "system_accent":
      return r("#4cc2ff");
    case "bar_info":
      return r({ monitor: 1, primary: true });
    case "get_active":
      return r(102);
    case "get_icon": {
      const p = String(args.path);
      const name = p.split(/[\\/]/).pop()!.replace(/\.exe$/i, "");
      return r(letterIcon(name.charAt(0).toUpperCase() + name.slice(1)));
    }
    case "list_apps":
      return r(apps);
    case "get_settings":
      return r(structuredClone(settings));
    case "save_settings":
      settings = args.settings as typeof settings;
      window.dispatchEvent(new CustomEvent("mock:settings-changed", { detail: structuredClone(settings) }));
      return r(undefined);
    case "system_status":
      return r({
        battery: { percent: 76, charging: false },
        online: true,
        volume,
        muted,
        cpu: 8 + Math.random() * 20,
        ram: 54,
        accent: "#4cc2ff",
      });
    case "set_volume":
      if (typeof args.volume === "number") volume = args.volume;
      if (typeof args.muted === "boolean") muted = args.muted;
      return r(undefined);
    case "media_info":
      return r({ title: "Midnight City", artist: "M83", playing, source: "Spotify.exe" });
    case "media_action":
      if (args.action === "toggle") playing = !playing;
      return r(undefined);
    case "update_status":
    case "check_update":
      return r({
        current: "0.2.0",
        // `?update`: pretend a new version is out
        available: new URLSearchParams(location.search).has("update")
          ? { version: "0.3.0", notes: "Nuovo tema, anteprime più veloci e correzioni varie." }
          : null,
      });
    case "install_update": {
      // fake download, then "fail" so the page stays usable in design mode
      const total = 6_000_000;
      for (let done = 0; done <= total; done += 600_000) {
        window.dispatchEvent(new CustomEvent("mock:update-progress", { detail: { done, total } }));
        await new Promise((ok) => setTimeout(ok, 150));
      }
      throw "modalità design: niente installazione";
    }
    default:
      console.info("[mock]", cmd, args);
      return r(null);
  }
}
