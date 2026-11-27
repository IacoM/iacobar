// Minimal stroke icons (24x24 viewBox), rendered with {@html}.
const s = (d: string) =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">${d}</svg>`;

export const ico = {
  start: `<svg viewBox="0 0 24 24"><rect x="3" y="3" width="8.2" height="8.2" rx="2.2" fill="currentColor"/><rect x="12.8" y="3" width="8.2" height="8.2" rx="2.2" fill="currentColor" opacity=".75"/><rect x="3" y="12.8" width="8.2" height="8.2" rx="2.2" fill="currentColor" opacity=".75"/><rect x="12.8" y="12.8" width="8.2" height="8.2" rx="2.2" fill="currentColor" opacity=".5"/></svg>`,
  search: s(`<circle cx="11" cy="11" r="6.5"/><path d="m20 20-4.2-4.2"/>`),
  taskview: s(`<rect x="3" y="5" width="13" height="10" rx="2"/><path d="M8 19h11a2 2 0 0 0 2-2V9"/>`),
  wifi: s(`<path d="M2.5 9a14 14 0 0 1 19 0"/><path d="M5.5 12.5a9.5 9.5 0 0 1 13 0"/><path d="M8.7 16a5 5 0 0 1 6.6 0"/><circle cx="12" cy="19.2" r=".9" fill="currentColor"/>`),
  offline: s(`<path d="M2.5 9a14 14 0 0 1 19 0" opacity=".35"/><path d="M5.5 12.5a9.5 9.5 0 0 1 13 0" opacity=".35"/><path d="m4 4 16 16"/>`),
  vol0: s(`<path d="M4 9.5h3L11.5 6v12L7 14.5H4z"/><path d="m16 10 4 4m0-4-4 4"/>`),
  vol1: s(`<path d="M4 9.5h3L11.5 6v12L7 14.5H4z"/><path d="M15 9.5a3.5 3.5 0 0 1 0 5"/>`),
  folder: s(`<path d="M3.5 7.5a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v7.5a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>`),
  file: s(`<path d="M7 3.5h6.5L18 8v11.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 6 19.5V5A1.5 1.5 0 0 1 7 3.5z"/><path d="M13 3.5V8h5"/>`),
  bolt: s(`<path d="M13 3 5.5 13.5H12L11 21l7.5-10.5H12z"/>`),
  check: s(`<path d="m5 12.5 4.5 4.5L19 7.5"/>`),
  speaker: s(`<rect x="6" y="3" width="12" height="18" rx="2.5"/><circle cx="12" cy="14" r="3"/><circle cx="12" cy="7.5" r="1"/>`),
  vol2: s(`<path d="M4 9.5h3L11.5 6v12L7 14.5H4z"/><path d="M15 9.5a3.5 3.5 0 0 1 0 5"/><path d="M17.8 7a7 7 0 0 1 0 10"/>`),
  bell: s(`<path d="M6 16V11a6 6 0 1 1 12 0v5l1.5 2h-15z"/><path d="M10 20.5a2 2 0 0 0 4 0"/>`),
  play: `<svg viewBox="0 0 24 24"><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" fill="currentColor"/></svg>`,
  pause: `<svg viewBox="0 0 24 24"><rect x="6.5" y="5" width="4" height="14" rx="1.2" fill="currentColor"/><rect x="13.5" y="5" width="4" height="14" rx="1.2" fill="currentColor"/></svg>`,
  next: `<svg viewBox="0 0 24 24"><path d="M5 6.3v11.4a1 1 0 0 0 1.5.86L15 13v4.5a1 1 0 0 0 2 0v-11a1 1 0 0 0-2 0V11L6.5 5.44A1 1 0 0 0 5 6.3z" fill="currentColor"/></svg>`,
  prev: `<svg viewBox="0 0 24 24"><path d="M19 6.3v11.4a1 1 0 0 1-1.5.86L9 13v4.5a1 1 0 0 1-2 0v-11a1 1 0 0 1 2 0V11l8.5-5.56A1 1 0 0 1 19 6.3z" fill="currentColor"/></svg>`,
  power: s(`<path d="M12 3v8"/><path d="M6.4 6.6a8 8 0 1 0 11.2 0"/>`),
  restart: s(`<path d="M20 12a8 8 0 1 1-2.4-5.7"/><path d="M20 4v4.5h-4.5"/>`),
  lock: s(`<rect x="5" y="10.5" width="14" height="10" rx="2.5"/><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5"/>`),
  logout: s(`<path d="M14 4h4a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-4"/><path d="M10 16l-4-4 4-4"/><path d="M6 12h10"/>`),
  moon: s(`<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>`),
  gear: s(`<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z"/>`),
  close: s(`<path d="M6 6l12 12M18 6 6 18"/>`),
  pin: s(`<path d="M9 4h6l-1 5 3 3v2H7v-2l3-3z"/><path d="M12 14v6"/>`),
  unpin: s(`<path d="M9 4h6l-1 5 3 3v2H7v-2l3-3z"/><path d="M12 14v6"/><path d="m4 4 16 16"/>`),
  plus: s(`<path d="M12 5v14M5 12h14"/>`),
  minimize: s(`<path d="M5 12h14"/>`),
  chevL: s(`<path d="m15 6-6 6 6 6"/>`),
  chevR: s(`<path d="m9 6 6 6-6 6"/>`),
  chevD: s(`<path d="m6 9 6 6 6-6"/>`),
  chevU: s(`<path d="m6 15 6-6 6 6"/>`),
  globe: s(`<circle cx="12" cy="12" r="9"/><path d="M3 12h18M12 3a14 14 0 0 1 0 18M12 3a14 14 0 0 0 0 18"/>`),
  calc: s(`<rect x="5" y="3" width="14" height="18" rx="2.5"/><path d="M8.5 7h7M8.5 12h.01M12 12h.01M15.5 12h.01M8.5 16h.01M12 16h.01M15.5 16h.01"/>`),
  bluetooth: s(`<path d="m7 7 10 10-5 4V3l5 4L7 17"/>`),
  sliders: s(`<path d="M4 7h10M18 7h2M4 17h4M12 17h8"/><circle cx="16" cy="7" r="2"/><circle cx="10" cy="17" r="2"/>`),
  taskmgr: s(`<path d="M3 12h4l2.5-6 4 12 2.5-6H21"/>`),
  windows: s(`<rect x="3" y="4" width="18" height="16" rx="2.5"/><path d="M3 8.5h18"/>`),
};

export function batteryIcon(percent: number, charging: boolean): string {
  const w = Math.max(1, Math.round((percent / 100) * 13));
  const fill = percent <= 15 && !charging ? "#ff6b6b" : "currentColor";
  const bolt = charging
    ? `<path d="M11.8 8.2 9.6 12.3h2.8l-1.1 3.5 3-4.6h-2.7z" fill="var(--bar-bg-solid, #222)" stroke="none"/>`
    : "";
  return `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><rect x="2.5" y="7" width="17" height="10" rx="2.6"/><path d="M21.5 10.5v3" stroke-linecap="round"/><rect x="4.5" y="9" width="${w}" height="6" rx="1.2" fill="${fill}" stroke="none"/>${bolt}</svg>`;
}

export function volumeIcon(volume: number, muted: boolean): string {
  if (muted || volume <= 0.001) return ico.vol0;
  return volume < 0.5 ? ico.vol1 : ico.vol2;
}

/** App mark "Dock" in colour, simplified to read at ~30px:
 *  a bar with two apps and the active one raised in the middle. */
export const logoGlyph = `<svg viewBox="0 0 24 24"><defs><linearGradient id="lgMain" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#a77bff"/><stop offset="1" stop-color="#22d3ee"/></linearGradient><linearGradient id="lgL" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6ff2dc"/><stop offset="1" stop-color="#14b8a6"/></linearGradient><linearGradient id="lgR" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ff9ee0"/><stop offset="1" stop-color="#e5469c"/></linearGradient></defs><rect x="0.8" y="12.6" width="22.4" height="9" rx="4.5" fill="#3b3d62" stroke="#ffffff" stroke-opacity=".22" stroke-width=".6"/><rect class="d-side d-l" x="2.7" y="14.3" width="5.6" height="5.6" rx="1.8" fill="url(#lgL)"/><rect class="d-side d-r" x="15.7" y="14.3" width="5.6" height="5.6" rx="1.8" fill="url(#lgR)"/><g class="d-main"><rect x="6.9" y="2.6" width="10.2" height="10.2" rx="3.1" fill="url(#lgMain)" stroke="#ffffff" stroke-opacity=".35" stroke-width=".5"/></g><rect class="d-ind" x="9.8" y="18.8" width="4.4" height="1.5" rx=".75" fill="#7dd3fc"/></svg>`;
