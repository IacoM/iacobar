import { prettyExe, type Pinned, type WinInfo } from "./api";

export interface Group {
  appId: string;
  name: string;
  /** What to launch for a new instance. */
  target: string;
  /** Path used to extract the icon. */
  iconPath: string;
  pinned: boolean;
  windows: WinInfo[];
}

/** Pinned apps first (in the user's order), then other running apps. */
export function buildGroups(pinned: Pinned[], windows: WinInfo[]): Group[] {
  const groups: Group[] = pinned.map((p) => ({
    appId: p.appId.toLowerCase(),
    name: p.name,
    target: p.target,
    iconPath: p.appId || p.target,
    pinned: true,
    windows: [],
  }));
  const byId = new Map(groups.map((g) => [g.appId, g]));
  // pinned Store apps ("shell:appsfolder\spotifyab.spotifymusic_zpdnek...!spotify")
  // run from "...\windowsapps\spotifyab.spotifymusic_<ver>_x64__zpdnek...\spotify.exe"
  const packaged = groups.flatMap((g) => {
    const m = g.appId.match(/^shell:appsfolder\\([^_\\!]+)_([^_\\!]+)!/);
    return m ? [{ g, name: `\\windowsapps\\${m[1]}_`, pub: `__${m[2]}\\` }] : [];
  });
  const packagedGroup = (exe: string) => {
    const e = exe.toLowerCase();
    return packaged.find((p) => e.includes(p.name) && e.includes(p.pub))?.g;
  };
  for (const w of windows) {
    let g = byId.get(w.appId) ?? packagedGroup(w.exe);
    if (!g) {
      g = {
        appId: w.appId,
        name: prettyExe(w.exe) || w.title,
        target: w.exe,
        iconPath: w.exe,
        pinned: false,
        windows: [],
      };
      byId.set(w.appId, g);
      groups.push(g);
    }
    g.windows.push(w);
  }
  return groups;
}
