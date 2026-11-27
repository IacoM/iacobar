<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { flip } from "svelte/animate";
  import { fly, scale } from "svelte/transition";
  import { backOut, cubicOut } from "svelte/easing";
  import { api, isTauri, listen, type Desktops, type TrayItem, type BarInfo, type MediaInfo, type Settings, type Shape, type SystemStatus, type WinInfo } from "$lib/api";
  import { buildGroups, type Group } from "$lib/groups";
  import { batteryIcon, ico, logoGlyph, volumeIcon } from "$lib/icons";
  import AppIcon from "$lib/components/AppIcon.svelte";
  import { setSystemAccent } from "$lib/theme";
  import { initUpdate, update } from "$lib/update.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";

  let windows = $state<WinInfo[]>([]);
  let active = $state(0);
  let settings = $state<Settings | null>(null);
  let status = $state<SystemStatus | null>(null);
  let media = $state<MediaInfo | null>(null);
  let art = $state<string | null>(null); // cover of the current song
  let desks = $state<Desktops>({ names: [], current: 0 }); // virtual desktops
  let now = $state(new Date());
  // macOS-style launch bounce: an icon keeps bouncing while its app starts
  // and lands (at the end of a hop, never mid-air) once a window shows up
  interface Bounce {
    until: number; // hard stop (app never opened)
    left: number; // minimum hops still to do
  }
  let bouncing = $state<Record<string, Bounce>>({});
  let ripple = $state<string | null>(null);
  let mouseX = $state<number | null>(null);
  let bar = $state<BarInfo | null>(null);
  let dockEl = $state<HTMLElement>();
  let volFlash = $state(false);
  let trayCount = $state(0);
  let collapsed = $state(false);
  let dense = $state(false); // many apps: smaller dock icons
  let crowded = $state(false); // still too many: the dock scrolls
  let flyoutShown = false;
  let hovering = false;
  let hideTimer = 0;
  let autoCollapsed = false;
  let volTimer = 0;

  // each monitor's bar shows the windows that live on that monitor
  const visibleWindows = $derived(
    !bar || settings?.allMonitors ? windows : windows.filter((w) => w.monitor === bar!.monitor),
  );
  const groups = $derived(buildGroups(settings?.pinned ?? [], visibleWindows));
  // fixed (default): blurred strip that reserves its space; floating: islands over the windows
  const fixed = $derived(!settings?.floating);

  // dock icon sizes, CSS px: [normal, focused (a bit bigger, real size: not a
  // blurry scale), crowded dock, crowded + focused], per "Dimensione icone"
  const SIZES = { small: [28, 32, 24, 27], medium: [32, 37, 26, 30], large: [36, 41, 30, 34] } as const;
  const sizes = $derived(SIZES[settings?.iconSize ?? "medium"] ?? SIZES.medium);
  const ICON = $derived(sizes[0]);
  const ICON_ACTIVE = $derived(sizes[1]);
  const ICON_DENSE = $derived(sizes[2]);
  const ICON_ACTIVE_DENSE = $derived(sizes[3]);
  const RADIUS = { square: 10, medium: 18, round: 24 } as const;

  // ---------------------------------------------------------------- data --

  onMount(() => {
    initUpdate();
    const un: Array<Promise<() => void>> = [];
    api.getWindows().then((w) => {
      windows = w;
      windowsReady = true;
    });
    api.getActive().then((a) => (active = a));
    api.getSettings().then((s) => (settings = s));
    api.barInfo().then((b) => (bar = b));
    un.push(listen<WinInfo[]>("windows-changed", (w) => (windows = w)));
    un.push(
      listen<number>("active-changed", (a) => {
        active = a;
        // switching desktops changes the foreground window: refresh the indicator
        if (desks.names.length > 1) api.desktops().then((d) => (desks = d));
      }),
    );
    api.desktops().then((d) => (desks = d));    un.push(listen<Desktops>("desktops-changed", (d) => (desks = d)));
    un.push(listen<Settings>("settings-changed", (s) => (settings = s)));
    un.push(listen("flyout-shown", () => (flyoutShown = true)));
    api.trayIcons().then((t) => (trayCount = t.length));
    un.push(listen<TrayItem[]>("tray-changed", (t) => (trayCount = t.length)));
    if (isTauri) un.push(followFileDrags());
    un.push(listen("flyout-hidden", () => {
      flyoutShown = false;
      scheduleAutoHide();
    }));

    const pollStatus = async () => {
      status = await api.systemStatus();
      setSystemAccent(status?.accent); // used only with the "Windows" accent
    };
    // the media widget lives on the primary bar only: others don't poll
    let artKey = "";
    const pollMedia = async () => {
      if (settings?.showMedia !== false && bar?.primary !== false) {
        media = await api.mediaInfo();
        // cover art: fetched once per song
        const key = media ? `${media.title}|${media.artist}` : "";
        if (key !== artKey) {
          artKey = key;
          art = key ? await api.mediaArt().catch(() => null) : null;
        }
      }
    };
    pollStatus();
    pollMedia();
    const t1 = setInterval(pollStatus, 3000);
    const t2 = setInterval(pollMedia, 2000);
    // wake up once per minute, on the minute (every second only when the
    // clock shows seconds)
    tick();
    // re-layout only when an island actually changes size (no polling)
    const ro = new ResizeObserver(() => requestAnimationFrame(layoutTick));
    const observeIslands = () => document.querySelectorAll(".island").forEach((el) => ro.observe(el));
    observeIslands();
    // apps appearing/disappearing: re-measure (a pinned min-width would
    // otherwise keep the dock from ever shrinking back)
    const mo = new MutationObserver(() => {
      observeIslands();
      requestAnimationFrame(layoutTick);
    });
    mo.observe(document.querySelector(".bar")!, { childList: true, subtree: true });
    window.addEventListener("resize", layoutTick);
    layoutTick();
    const t4 = setTimeout(layoutTick, 800); // after the intro animation
    return () => {
      clearInterval(t1);
      clearInterval(t2);
      clearTimeout(clockTimer);
      clearTimeout(t4);
      ro.disconnect();
      mo.disconnect();
      window.removeEventListener("resize", layoutTick);
      un.forEach((p) => p.then((f) => f()));
    };
  });

  // ------------------------------------------------ pixel snap + regions --

  /** Keeps the centred dock on whole device pixels (no blurry half-pixel
   *  positions) and tells Windows which parts of the bar catch the mouse. */
  function layoutTick() {
    updateDensity();
    if (dockEl) {
      // measure the dock's NATURAL width (without our own min-width, or it
      // could never shrink again), then pad it by one device pixel if needed
      // so that the centred dock lands on whole pixels
      const dpr = window.devicePixelRatio || 1;
      const prev = dockEl.style.minWidth;
      dockEl.style.minWidth = "";
      const w = dockEl.getBoundingClientRect().width;
      const odd = Math.round((window.innerWidth - w) * dpr) % 2 !== 0;
      const next = odd && !crowded ? `${w + 1 / dpr}px` : "";
      dockEl.style.minWidth = next;
      if (next !== prev) requestAnimationFrame(() => isTauri && syncRegion());
    }
    if (isTauri) syncRegion();
  }

  /** Many apps open: smaller icons first, then a scrollable dock, so the
   *  dock never covers the side islands. */
  function updateDensity() {
    if (!dockEl) return;
    const barEl = dockEl.parentElement!;
    const cs = getComputedStyle(barEl);
    const gap = parseFloat(cs.columnGap) || 0;
    const pad = parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    const side = (sel: string) => document.querySelector<HTMLElement>(sel)?.offsetWidth ?? 0;
    const avail = window.innerWidth - pad - 2 * gap - side(".slot.left .island") - side(".slot.right .island");
    const apps = dockEl.querySelectorAll(".app").length;
    const w = dockEl.scrollWidth; // content width, even while it scrolls
    if (!dense && w > avail) dense = true;
    // each app is ~10px wider at normal size: only grow back when it fits
    else if (dense && w + apps * 10 < avail - 8) dense = false;
    crowded = w > avail + 1;
  }

  // Only the islands catch the mouse: clicks in the gaps reach the apps below.
  /** Layout box of an element, ignoring CSS transforms (animations). */
  function box(el: HTMLElement) {
    let x = 0, y = 0;
    for (let n: HTMLElement | null = el; n; n = n.offsetParent as HTMLElement | null) {
      x += n.offsetLeft;
      y += n.offsetTop;
    }
    return { left: x, top: y, width: el.offsetWidth, height: el.offsetHeight };
  }

  let lastRegion = "";
  function syncRegion() {
    if (fixed) {
      // the fixed bar owns its whole strip (and the blur must cover it all)
      if (lastRegion !== "fixed") {
        lastRegion = "fixed";
        api.setRegion([]);
      }
      return;
    }
    const shapes: Shape[] = [];
    if (collapsed) {
      // only the handle (and the bottom edge in auto-hide) catch the mouse
      // the pill, at its hover (widest) size, centred on the screen: the
      // region also clips drawing, so it must cover the grown pill too
      // EXACTLY the pill's shape: transparent pixels inside a window region
      // get painted white on some monitors (a white halo)
      const W = 152, H = 24, B = 4;
      shapes.push({ x: (window.innerWidth - W) / 2, y: window.innerHeight - B - H, w: W, h: H, r: H / 2 });
      if (settings?.autoHide) shapes.push({ x: 0, y: window.innerHeight - 3, w: window.innerWidth, h: 3, r: 0 });
    }
    // same corner radius as the islands ("Angoli delle isole")
    const radius = RADIUS[settings?.roundness ?? "medium"] ?? 18;
    document.querySelectorAll<HTMLElement>(collapsed ? ".none" : ".island").forEach((el) => {
      const r = box(el);
      shapes.push({ x: r.left, y: r.top, w: r.width, h: r.height, r: Math.min(radius, r.height / 2) });
      if (el.classList.contains("dock")) {
        // room above the dock for the magnified icons
        shapes.push({ x: r.left + 18, y: 0, w: r.width - 36, h: r.top + 18, r: 0 });
      }
    });
    const key = JSON.stringify(shapes.map((s) => [s.x, s.y, s.w, s.h, s.r].map(Math.round)));
    if (key !== lastRegion && shapes.length) {
      lastRegion = key;
      api.setRegion(shapes);
    }
  }

  // corner radius changed in the settings: the region must follow
  $effect(() => {
    void settings?.roundness;
    untrack(() => requestAnimationFrame(() => isTauri && syncRegion()));
  });

  // ------------------------------------------------------------ previews --

  let hoverTimer = 0;
  let previewOpen = false;
  const CARD_W = 240;
  const CARD_H = 178;

  let closedTimer = 0;
  function enterGroup(e: MouseEvent, g: Group) {
    clearTimeout(hoverTimer);
    clearTimeout(closedTimer); // still in the "previews open" streak
    if (!g.windows.length) return;
    const x = centerOf(e);
    const data = { name: g.name, iconPath: g.iconPath, windows: $state.snapshot(g.windows).slice(0, 5) };
    const n = data.windows.length;
    hoverTimer = setTimeout(
      () => {
        previewOpen = true;
        api.showPreview(x, 24 + n * CARD_W + (n - 1) * 10, 24 + CARD_H, data);
      },
      previewOpen ? 60 : 380,
    ) as unknown as number;
  }

  function leaveGroup() {
    clearTimeout(hoverTimer);
    api.hidePreview(220);
    clearTimeout(closedTimer);
    closedTimer = setTimeout(() => (previewOpen = false), 260) as unknown as number;
  }

  // ------------------------------------------------------------ collapse --

  function setCollapsed(v: boolean) {
    clearTimeout(hideTimer);
    if (collapsed === v) return;
    collapsed = v;
    autoCollapsed = false;
    if (v) {
      mouseX = null;
      api.hidePreview(0);
    } else if (settings?.autoHide && !hovering) {
      // revealed from the screen edge: tuck away again if the pointer
      // never reaches the islands
      hideTimer = setTimeout(() => !hovering && !flyoutShown && setCollapsed(true), 1800) as unknown as number;
    }
    requestAnimationFrame(syncRegion);
  }

  function scheduleAutoHide() {
    clearTimeout(hideTimer);
    if (fixed || !settings?.autoHide || hovering || flyoutShown) return;
    hideTimer = setTimeout(() => {
      setCollapsed(true);
      autoCollapsed = true;
    }, 900) as unknown as number;
  }

  $effect(() => {
    // react ONLY to the auto-hide setting being switched (untrack: reading
    // `collapsed` here must not re-run this when the bar is hidden by hand)
    const auto = settings?.autoHide;
    const isFixed = fixed;
    untrack(() => {
      if (isFixed) {
        setCollapsed(false);
        lastRegion = "";
        requestAnimationFrame(syncRegion);
        return;
      }
      if (auto) scheduleAutoHide();
      else if (collapsed && autoCollapsed) setCollapsed(false);
    });
  });

  /** Wheel down on an island's empty area = swipe the bar away. */
  function swipe(e: WheelEvent) {
    if (!fixed && e.target === e.currentTarget && e.deltaY > 0) setCollapsed(true);
  }

  /** Wheel on the dock: scroll it when it is too full, else swipe away. */
  function dockWheel(e: WheelEvent) {
    if (crowded && dockEl) {
      dockEl.scrollLeft += e.deltaY || e.deltaX;
      mouseX = null;
      return;
    }
    swipe(e);
  }

  // ------------------------------------------------------- drag reorder --
  // Press and drag an app sideways to move it; dropping an unpinned app
  // pins it at that spot. Order is saved in the settings.

  interface Drag {
    appId: string;
    from: number;
    over: number;
    startX: number;
    dx: number;
    centers: number[];
    step: number;
    active: boolean;
  }
  let drag = $state<Drag | null>(null);
  let suppressClick = false;

  function appButtons() {
    return [...(dockEl?.querySelectorAll<HTMLElement>(".app") ?? [])];
  }

  function dragStart(e: PointerEvent, g: Group, index: number) {
    if (e.button !== 0 || g.appId.startsWith("hwnd:")) return;
    const els = appButtons();
    const centers = els.map((el) => {
      const r = el.getBoundingClientRect();
      return r.left + r.width / 2;
    });
    const step = centers.length > 1 ? (centers[centers.length - 1] - centers[0]) / (centers.length - 1) : 46;
    drag = { appId: g.appId, from: index, over: index, startX: e.clientX, dx: 0, centers, step, active: false };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function dragMove(e: PointerEvent) {
    if (!drag) return;
    drag.dx = e.clientX - drag.startX;
    if (!drag.active && Math.abs(drag.dx) > 6) {
      drag.active = true;
      clearTimeout(hoverTimer);
      api.hidePreview(0);
      mouseX = null;
    }
    if (drag.active) {
      const x = drag.centers[drag.from] + drag.dx;
      let best = 0;
      drag.centers.forEach((c, i) => {
        if (Math.abs(c - x) < Math.abs(drag!.centers[best] - x)) best = i;
      });
      drag.over = best;
    }
  }

  async function dragEnd() {
    const d = drag;
    if (!d?.active || !settings || d.over === d.from) {
      drag = null;
      if (d?.active) {
        suppressClick = true;
        setTimeout(() => (suppressClick = false), 50);
      }
      return;
    }
    suppressClick = true; // the pointerup also produces a click
    setTimeout(() => (suppressClick = false), 50);
    const order = groups.map((g) => g.appId);
    const [moved] = order.splice(d.from, 1);
    order.splice(d.over, 0, moved);
    const byId = new Map(groups.map((g) => [g.appId, g]));
    const pinnedNow = new Set(settings.pinned.map((p) => p.appId.toLowerCase()));
    const pinned = order
      .filter((id) => pinnedNow.has(id) || id === d.appId)
      .map((id) => {
        const old = settings!.pinned.find((p) => p.appId.toLowerCase() === id);
        const g = byId.get(id)!;
        return old ?? { name: g.name, target: g.target, appId: g.appId };
      });
    const next = { ...$state.snapshot(settings), pinned };
    // same tick: the new order and the end of the drag offsets land together,
    // so `flip` animates every icon from where it is right now
    settings = next;
    drag = null;
    await api.saveSettings(next);
  }

  /** Visual offset of each app while dragging. */
  function shiftOf(i: number): string {
    if (!drag?.active) return "";
    const { from, over, dx, step } = drag;
    if (i === from) return `transform: translateX(${dx}px) scale(1.12); z-index: 3;`;
    if (from < over && i > from && i <= over) return `transform: translateX(${-step}px);`;
    if (from > over && i >= over && i < from) return `transform: translateX(${step}px);`;
    return "";
  }

  // ---------------------------------------------------------- file drops --
  // Dragging files from Explorer over the dock: hovering an app brings its
  // window forward (drop the file into it), dropping on the icon opens the
  // files with that app.

  let dropTarget = $state<string | null>(null);
  let dropTimer = 0;

  function groupAt(px: number, py: number): Group | null {
    const dpr = window.devicePixelRatio || 1;
    const el = document.elementFromPoint(px / dpr, py / dpr)?.closest<HTMLElement>(".app");
    const id = el?.dataset.app;
    return id ? (groups.find((g) => g.appId === id) ?? null) : null;
  }

  function openWith(g: Group, paths: string[]) {
    const quoted = paths.map((p) => `"${p}"`).join(" ");
    startBounce(g.appId);
    if (g.target.toLowerCase().startsWith("shell:")) paths.forEach((p) => api.launch(p));
    else api.launch(g.target, quoted);
  }

  function followFileDrags(): Promise<() => void> {
    return getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "leave") {
        clearTimeout(dropTimer);
        dropTarget = null;
        return;
      }
      const g = groupAt(p.position.x, p.position.y);
      if (p.type === "drop") {
        clearTimeout(dropTimer);
        dropTarget = null;
        if (g && p.paths.length) openWith(g, p.paths);
        return;
      }
      // enter / over
      const id = g?.appId ?? null;
      if (id === dropTarget) return;
      dropTarget = id;
      clearTimeout(dropTimer);
      if (g?.windows.length) {
        const hwnd = (g.windows.find((w) => w.hwnd === active) ?? g.windows[0]).hwnd;
        dropTimer = setTimeout(() => api.windowAction(hwnd, "activate"), 700) as unknown as number;
      }
    });
  }

  // ------------------------------------------------------------- actions --

  function centerOf(e: Event): number {
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    return r.left + r.width / 2;
  }

  function flyout(e: Event, panel: string, w: number, h: number, data?: unknown) {
    api.showFlyout(panel, centerOf(e), w, h, data);
  }

  function trayFlyout(e: Event) {
    const rows = Math.max(1, Math.ceil(trayCount / 5));
    flyout(e, "tray", 214, 20 + rows * 38 - 4 + 22, undefined);
  }

  function groupFlyout(e: Event, g: Group) {
    const h = 64 + g.windows.length * 46 + 3 * 40 + 24;
    flyout(e, "group", 320, Math.min(h, 560), $state.snapshot(g));
  }

  function startBounce(id: string, hops = 1, maxMs = 10_000) {
    bouncing[id] = { until: Date.now() + maxMs, left: hops };
  }

  /** Called at the end of every hop: keep going or land. */
  function hopDone(g: Group) {
    const b = bouncing[g.appId];
    if (!b) return;
    b.left--;
    if ((g.windows.length > 0 && b.left <= 0) || Date.now() > b.until) delete bouncing[g.appId];
  }

  // apps opened elsewhere (Start, a shortcut...) hop twice when they arrive
  let seen: Map<string, number> | null = null;
  let windowsReady = false; // don't bounce what was already open at startup
  $effect(() => {
    const counts = new Map(groups.map((g) => [g.appId, g.windows.length]));
    untrack(() => {
      if (!windowsReady) return;
      if (seen) {
        for (const [id, n] of counts) {
          if (n > 0 && !seen.get(id) && !bouncing[id]) startBounce(id, 2, 2_000);
        }
      }
      seen = counts;
    });
  });

  function clickGroup(e: MouseEvent, g: Group) {
    if (suppressClick) return;
    if (g.windows.length === 0) {
      startBounce(g.appId);
      ripple = g.appId;
      setTimeout(() => ripple === g.appId && (ripple = null), 800);
      api.launch(g.target);
    } else if (g.windows.length === 1) {
      api.windowAction(g.windows[0].hwnd, "toggle");
    } else {
      groupFlyout(e, g);
    }
  }

  function auxGroup(e: MouseEvent, g: Group) {
    if (e.button === 1) {
      e.preventDefault();
      startBounce(g.appId);
      api.launch(g.target);
    }
  }

  function wheelGroup(e: WheelEvent, g: Group) {
    if (g.windows.length < 2) return;
    e.stopPropagation(); // cycling this app's windows, not scrolling the dock
    const i = g.windows.findIndex((w) => w.hwnd === active);
    const next = (i + (e.deltaY > 0 ? 1 : -1) + g.windows.length) % g.windows.length;
    api.windowAction(g.windows[next].hwnd, "activate");
  }

  function wheelVolume(e: WheelEvent) {
    if (!status) return;
    const v = Math.min(1, Math.max(0, status.volume + (e.deltaY < 0 ? 0.02 : -0.02)));
    status.volume = v;
    api.setVolume(v);
    volFlash = true;
    clearTimeout(volTimer);
    volTimer = setTimeout(() => (volFlash = false), 1200) as unknown as number;
  }

  // Dock "wave" magnification: icons near the pointer grow. At rest no
  // transform is applied, so icons are drawn pixel-exact.
  function magnify(node: HTMLElement) {
    $effect(() => {
      if (mouseX === null) {
        node.style.setProperty("--m", "0");
        node.classList.remove("mag");
        return;
      }
      const r = node.getBoundingClientRect();
      const m = Math.max(0, 1 - Math.abs(mouseX - (r.left + r.width / 2)) / 110);
      node.style.setProperty("--m", m.toFixed(3));
      node.classList.toggle("mag", m > 0);
    });
  }

  function spotlight(e: MouseEvent) {
    mouseX = e.clientX;
    const el = e.currentTarget as HTMLElement;
    el.style.setProperty("--mx", `${e.clientX - el.getBoundingClientRect().left}px`);
  }

  // --------------------------------------------------------------- views --

  let clockTimer = 0;
  function tick() {
    clearTimeout(clockTimer);
    now = new Date();
    const step = settings?.clockSeconds ? 1000 : 60_000;
    clockTimer = setTimeout(tick, step - (Date.now() % step) + 20) as unknown as number;
  }
  // seconds switched on/off: re-time the clock right away
  $effect(() => {
    void settings?.clockSeconds;
    untrack(tick);
  });

  const time = $derived(
    now.toLocaleTimeString("it-IT", { hour: "2-digit", minute: "2-digit", second: settings?.clockSeconds ? "2-digit" : undefined }),
  );
  const day = $derived(now.toLocaleDateString("it-IT", { weekday: "short", day: "numeric", month: "short" }));
  const isActive = (g: Group) => g.windows.some((w) => w.hwnd === active);
  const tooltip = (g: Group) =>
    g.windows.length ? g.windows.map((w) => w.title).join("\n") : `${g.name}\nClic per avviare`;
  const ring = (p: number) => `${Math.max(0, Math.min(100, p)) * 0.9425} 94.25`; // r=15 circumference
  // widgets live on the primary monitor's bar only
  // widgets (music, CPU/RAM) live on the primary monitor only
  const primaryHere = $derived(bar?.primary ?? true);
  const showMediaHere = $derived(primaryHere && settings?.showMedia !== false && !!media);
  const showStatsHere = $derived(primaryHere && !!settings?.showStats && !!status);
  const showLeft = $derived(showMediaHere || showStatsHere);
</script>

<svg width="0" height="0" style="position:absolute" aria-hidden="true">
  <defs>
    <linearGradient id="gaugeGrad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="var(--accent)" />
      <stop offset="1" stop-color="#b56bff" />
    </linearGradient>
  </defs>
</svg>

<div class="stage" class:mock={!isTauri} style="--r:{RADIUS[settings?.roundness ?? 'medium'] ?? 18}px">
  <nav
    class="bar"
    class:left-aligned={settings && !settings.centered}
    class:collapsed
    class:fixed
    onmouseenter={() => { hovering = true; clearTimeout(hideTimer); }}
    onmouseleave={() => { hovering = false; scheduleAutoHide(); }}
  >
    <!-- LEFT island: media + stats -->
    <div class="slot left">
      {#if showLeft}
        <section class="island widgets" onwheel={swipe}>
          {#if showMediaHere && media}
            <div class="media">
              <button class="media-main" onclick={(e) => flyout(e, "audio", 360, 390)} title="{media.title} — {media.artist}">
                <span class="disc" class:spin={media.playing} class:cover={!!art} style={art ? `background-image:url("${art}")` : ""}><i></i></span>
                <span class="media-text">
                  <b>{media.title}</b>
                  <small>{media.artist}</small>
                </span>
              </button>
              <button class="icon-btn mc" onclick={() => api.mediaAction("prev")} title="Precedente">{@html ico.prev}</button>
              <button class="icon-btn mc play" onclick={() => api.mediaAction("toggle")} title={media.playing ? "Pausa" : "Riproduci"}>
                {@html media.playing ? ico.pause : ico.play}
              </button>
              <button class="icon-btn mc" onclick={() => api.mediaAction("next")} title="Successivo">{@html ico.next}</button>
            </div>
          {/if}
          {#if showStatsHere && status}
            <button class="stats" onclick={() => api.shell("taskmgr")} title="CPU {Math.round(status.cpu)}% · RAM {status.ram}%&#10;Clic: Gestione attività">
              {#each [["CPU", status.cpu], ["RAM", status.ram]] as [label, val]}
                <span class="gauge" class:hot={+val > 85}>
                  <svg viewBox="0 0 36 36">
                    <circle cx="18" cy="18" r="15" class="track" />
                    <circle cx="18" cy="18" r="15" class="val" stroke-dasharray={ring(+val)} />
                  </svg>
                  <span class="gl"><b>{Math.round(+val)}</b><small>{label}</small></span>
                </span>
              {/each}
            </button>
          {/if}
        </section>
      {/if}
    </div>

    <!-- CENTER dock -->
    <section
      class="island dock"
      class:dense
      class:crowded
      role="toolbar"
      tabindex="-1"
      onwheel={dockWheel}
      bind:this={dockEl}
      onmousemove={spotlight}
      onmouseleave={() => (mouseX = null)}
    >
      <button
        class="logo"
        title="Start (clic destro: menu Win+X)"
        onclick={(e) => flyout(e, "start", 640, 620)}
        oncontextmenu={(e) => { e.preventDefault(); api.shell("winx"); }}
      >
        <span class="logo-glow"></span>
        <span class="logo-tile">{@html logoGlyph}</span>
        {#if update.available}<span class="upd-dot" title="Aggiornamento disponibile"></span>{/if}
      </button>
      <button class="tool" title="Cerca" onclick={(e) => flyout(e, "start", 640, 620, { focusSearch: true })}>
        {@html ico.search}
      </button>
      <button class="tool" title="Visualizzazione attività" onclick={() => api.shell("taskview")}>
        {@html ico.taskview}
      </button>
      {#if desks.names.length > 1}
        <div
          class="desks"
          role="group"
          aria-label="Desktop virtuali"
          title="{desks.names[desks.current]} (rotella: cambia desktop)"
          onwheel={(e) => {
            e.stopPropagation();
            const n = desks.names.length;
            const next = Math.min(n - 1, Math.max(0, desks.current + (e.deltaY > 0 ? 1 : -1)));
            if (next !== desks.current) {
              desks.current = next;
              api.switchDesktop(next);
            }
          }}
        >
          {#each desks.names as name, i (i)}
            <button
              class="desk"
              class:on={i === desks.current}
              title={name}
              aria-label={name}
              onclick={() => {
                desks.current = i;
                api.switchDesktop(i);
              }}
            ></button>
          {/each}
        </div>
      {/if}

      <span class="sep"><i></i><i></i></span>

      {#each groups as g, i (g.appId)}
        <button
          class="app"
          class:dragging={drag?.active && drag.from === i}
          class:shifting={drag?.active}
          style={shiftOf(i)}
          onpointerdown={(e) => dragStart(e, g, i)}
          onpointermove={dragMove}
          onpointerup={dragEnd}
          onpointercancel={() => (drag = null)}
          animate:flip={{ duration: 320, easing: cubicOut }}
          in:scale={{ duration: 340, start: 0.3, opacity: 0, easing: backOut }}
          out:scale={{ duration: 200, start: 0.5, opacity: 0 }}
          use:magnify
          class:running={g.windows.length > 0}
          class:active={isActive(g)}
          class:launching={!!bouncing[g.appId]}
          class:attention={!isActive(g) && g.windows.some((w) => w.attention)}
          class:drop-target={dropTarget === g.appId}
          data-app={g.appId}
          title={g.windows.length ? undefined : tooltip(g)}
          onmouseenter={(e) => !drag && enterGroup(e, g)}
          onmouseleave={leaveGroup}
          onclick={(e) => clickGroup(e, g)}
          onauxclick={(e) => auxGroup(e, g)}
          onmousedown={(e) => e.button === 1 && e.preventDefault()}
          oncontextmenu={(e) => { e.preventDefault(); groupFlyout(e, g); }}
          onwheel={(e) => wheelGroup(e, g)}
        >
          <span class="lift" onanimationiteration={() => hopDone(g)}>
            <AppIcon
              path={g.iconPath}
              hwnd={g.windows[0]?.hwnd}
              name={g.name}
              size={dense ? (isActive(g) ? ICON_ACTIVE_DENSE : ICON_DENSE) : isActive(g) ? ICON_ACTIVE : ICON}
            />
          </span>
          {#if settings?.showLabels && g.windows.length}
            <span class="label">{g.windows.find((w) => w.hwnd === active)?.title ?? g.windows[0].title}</span>
          {/if}
          <span class="ind" class:multi={g.windows.length > 1}></span>
          {#if ripple === g.appId}<span class="ripple"></span>{/if}
        </button>
      {/each}
    </section>

    <!-- RIGHT island: system + clock -->
    <div class="slot right">
      <section class="island sys" onwheel={swipe}>
        <button class="tool tray-btn" title="App in background ({trayCount})" onclick={trayFlyout}>
          {@html ico.chevU}
          {#if trayCount}<span class="badge">{trayCount}</span>{/if}
        </button>
        {#if status}
          <button class="chip" title="Rete, volume e batteria (rotella: volume)" onclick={(e) => flyout(e, "audio", 360, 390)} onwheel={wheelVolume}>
            <span class="g">{@html status.online ? ico.wifi : ico.offline}</span>
            <span class="g vol">
              {@html volumeIcon(status.volume, status.muted)}
              {#if volFlash}
                <span class="vbar" transition:scale={{ duration: 180, start: 0.6 }}><i style="width:{status.muted ? 0 : status.volume * 100}%"></i></span>
              {/if}
            </span>
            {#if status.battery}
              <span class="g">{@html batteryIcon(status.battery.percent, status.battery.charging)}</span>
              <small>{status.battery.percent}%</small>
            {/if}
          </button>
        {/if}
        <button class="clock" onclick={(e) => flyout(e, "calendar", 340, 480)} title={now.toLocaleDateString("it-IT", { dateStyle: "full" })}>
          <span class="time">
            {#if settings?.clockSeconds}
              <!-- no slide every second: just the digits -->
              <b>{time}</b>
            {:else}
              {#key time}<b in:fly={{ y: 10, duration: 380, easing: backOut }} out:fly={{ y: -10, duration: 220 }}>{time}</b>{/key}
            {/if}
          </span>
          <small>{day}</small>
        </button>
        <button class="tool" title="Notifiche" onclick={() => api.shell("notifications")}>{@html ico.bell}</button>
        {#if !fixed}
          <button class="tool hide-btn" title="Nascondi la barra (riaprila dalla maniglia in basso)" onclick={() => setCollapsed(true)}>{@html ico.chevD}</button>
        {/if}
        <button class="peek" title="Mostra desktop" aria-label="Mostra desktop" onclick={() => api.shell("desktop")}></button>
      </section>
    </div>
  </nav>

  <!-- shown while the (floating) bar is tucked away -->
  {#if !fixed}
  <button
    class="handle"
    class:show={collapsed}
    title="Mostra la barra"
    aria-label="Mostra la barra"
    onclick={() => setCollapsed(false)}
    onmouseenter={() => settings?.autoHide && setCollapsed(false)}
    onwheel={(e) => e.deltaY < 0 && setCollapsed(false)}
  ><span class="hchev">{@html ico.chevU}</span></button>
  {#if collapsed && settings?.autoHide}
    <div class="edge" role="presentation" onmouseenter={() => setCollapsed(false)}></div>
  {/if}
  {/if}
</div>

<style>
  @property --spin {
    syntax: "<angle>";
    inherits: false;
    initial-value: 0deg;
  }

  .stage {
    position: relative;
    height: 100vh;
    --island-h: 48px;
    --r: 18px;
    --aurora-1: var(--accent);
    /* --aurora-2/3 and the surfaces come from the theme (theme.ts) */
  }
  /* Browser design-mode: fake wallpaper, bar at the bottom */
  .stage.mock {
    display: flex;
    align-items: flex-end;
    background:
      radial-gradient(1200px 600px at 20% 10%, #3a5bd9 0%, transparent 60%),
      radial-gradient(900px 500px at 85% 30%, #b83cc6 0%, transparent 55%),
      linear-gradient(160deg, #10162f, #1d1036);
  }
  .stage.mock .bar {
    height: 64px;
  }
  .stage.mock .bar.fixed {
    height: 56px;
    backdrop-filter: blur(30px) saturate(150%);
  }

  /* ---- fixed bar: solid strip in the islands' own colour ---- */
  .bar.fixed {
    align-items: center;
    padding: 0 8px;
    background: linear-gradient(180deg, var(--strip-1), var(--strip-2));
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08);
  }
  /* less headroom in the fixed strip: gentler magnification */
  .bar.fixed .app:global(.mag) .lift {
    transform: translateY(calc(var(--m) * -1px)) scale(calc(1 + var(--m) * 0.14));
  }
  .bar.fixed .island {
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 2px 10px rgba(0, 0, 0, 0.35);
  }
  :global(:root[data-scheme="light"]) .bar.fixed {
    box-shadow: inset 0 1px 0 #fff, inset 0 2px 0 rgba(0, 0, 0, 0.04);
  }
  :global(:root[data-scheme="light"]) .bar.fixed .island {
    box-shadow:
      inset 0 1px 0 #fff,
      inset 0 0 0 1px rgba(0, 0, 0, 0.08),
      0 2px 8px rgba(0, 0, 0, 0.08);
  }

  .bar {
    height: 100%;
    width: 100%;
    display: grid;
    /* the side islands keep their full width; only the dock gives way */
    grid-template-columns: minmax(max-content, 1fr) minmax(0, auto) minmax(max-content, 1fr);
    align-items: end;
    gap: 10px;
    padding: 0 10px 8px;
  }
  .bar.left-aligned {
    grid-template-columns: minmax(0, auto) minmax(max-content, 1fr) max-content;
  }
  .bar.left-aligned .dock {
    order: -1;
  }
  .slot {
    display: flex;
  }
  .slot .island {
    flex: none;
    white-space: nowrap;
  }
  .slot.right {
    justify-content: flex-end;
  }

  /* ---- island surface: dark glass + grain + top sheen ---- */
  .island {
    position: relative;
    height: var(--island-h);
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 0 6px;
    border-radius: var(--r);
    isolation: isolate;
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='.9' numOctaves='2' stitchTiles='stitch'/%3E%3CfeColorMatrix values='0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 .05 0'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)'/%3E%3C/svg%3E"),
      linear-gradient(180deg, rgba(255, 255, 255, 0.07), rgba(255, 255, 255, 0) 45%),
      linear-gradient(180deg, var(--surface-1), var(--surface-2));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      inset 0 -10px 20px -12px color-mix(in srgb, var(--aurora-2) 35%, transparent);
    animation: rise 0.6s var(--ease);
  }
  .island {
    transition:
      transform 0.42s cubic-bezier(0.3, 0.9, 0.3, 1),
      opacity 0.3s ease;
  }
  .bar.collapsed .island {
    transform: translateY(calc(100% + 16px)) scale(0.96);
    opacity: 0;
    pointer-events: none;
  }
  .bar .dock { transition-delay: 0.03s; }
  .bar .sys { transition-delay: 0.06s; }

  /* "show the bar" pill of the tucked-away floating bar. Centred with auto
     margins (no translateX): the clickable region, computed from the layout
     box, then sits exactly on the drawn pill. */
  .handle {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 4px;
    margin: 0 auto;
    width: 152px; /* fixed: the window region is exactly this pill */
    height: 24px;
    display: grid;
    place-items: center;
    border-radius: 12px;
    color: var(--fg);
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.08), rgba(255, 255, 255, 0) 60%),
      linear-gradient(180deg, var(--surface-1), var(--surface-2));
    /* no outer glow / drop shadow: the window region clips them into a
       visible rectangle (a "halo") around the pill */
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--aurora-2) 55%, transparent),
      inset 0 0 10px color-mix(in srgb, var(--aurora-2) 25%, transparent);
    transform: translateY(40px);
    opacity: 0;
    pointer-events: none;
    transition:
      transform 0.4s cubic-bezier(0.3, 0.9, 0.3, 1) 0.1s,
      opacity 0.3s 0.1s,
      width 0.3s var(--ease),
      box-shadow 0.3s;
  }
  .handle::before {
    /* aurora line along the top edge */
    content: "";
    position: absolute;
    left: 18px;
    right: 18px;
    top: 0;
    height: 2px;
    border-radius: 2px;
    background: linear-gradient(90deg, transparent, var(--aurora-1), var(--aurora-2), var(--aurora-3), transparent);
  }
  .handle.show {
    transform: none;
    opacity: 1;
    pointer-events: auto;
  }
  .handle:hover {
    box-shadow:
      inset 0 0 0 1.5px color-mix(in srgb, var(--aurora-1) 80%, transparent),
      inset 0 0 14px color-mix(in srgb, var(--aurora-1) 35%, transparent);
  }
  .hchev {
    width: 16px;
    height: 16px;
    transition: transform 0.3s var(--ease);
  }
  .handle:hover .hchev {
    transform: translateY(-2px);
  }
  .edge {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 3px;
  }
  .tray-btn {
    position: relative;
  }
  .badge {
    position: absolute;
    top: 3px;
    right: 2px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    border-radius: 7px;
    font-size: 9px;
    font-weight: 700;
    line-height: 14px;
    text-align: center;
    color: #0b0b12;
    background: linear-gradient(135deg, var(--aurora-1), var(--aurora-2));
  }
  .hide-btn {
    width: 28px;
    padding: 7px 5px;
  }

  @keyframes rise {
    from {
      transform: translateY(70px);
      opacity: 0;
    }
  }
  :global(:root[data-scheme="light"]) .island {
    background:
      linear-gradient(180deg, rgba(255, 255, 255, 0.9), rgba(255, 255, 255, 0) 50%),
      linear-gradient(180deg, var(--surface-1), var(--surface-2));
    box-shadow:
      inset 0 1px 0 #fff,
      inset 0 0 0 1px rgba(0, 0, 0, 0.08);
  }

  /* ---- dock: rotating aurora border + pointer spotlight ---- */
  .dock {
    --mx: -200px;
    padding: 0 8px 0 6px;
    gap: 2px;
    min-width: 0;
  }
  .dock > :global(*) {
    flex: none;
  }
  /* many apps: compact buttons */
  .dock.dense .app {
    min-width: 36px;
    padding: 0 5px;
  }
  .dock.dense .app.active {
    padding: 0 3px;
  }
  /* still too many: the dock scrolls (wheel) with faded edges */
  .dock.crowded {
    overflow-x: auto;
    overflow-y: hidden;
    scrollbar-width: none;
    -webkit-mask: linear-gradient(90deg, transparent, #000 14px calc(100% - 22px), transparent);
    mask: linear-gradient(90deg, transparent, #000 14px calc(100% - 22px), transparent);
  }
  /* the border / spotlight layers would scroll away with the content */
  .dock.crowded::before,
  .dock.crowded::after {
    display: none;
  }
  .dock::before {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    padding: 1.2px;
    background: conic-gradient(
      from var(--spin),
      transparent 0deg,
      var(--aurora-1) 50deg,
      var(--aurora-2) 110deg,
      transparent 170deg,
      transparent 190deg,
      var(--aurora-3) 250deg,
      var(--aurora-1) 300deg,
      transparent 350deg
    );
    -webkit-mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    -webkit-mask-composite: xor;
    mask: linear-gradient(#000 0 0) content-box, linear-gradient(#000 0 0);
    mask-composite: exclude;
    opacity: 0.55;
    transition: opacity 0.4s;
    /* runs only while the pointer is on the dock: zero cost at rest */
    animation: spin 6s linear infinite paused;
    pointer-events: none;
  }
  .dock:hover::before {
    opacity: 0.9;
    animation-play-state: running;
  }
  .dock::after {
    content: "";
    position: absolute;
    inset: 0;
    z-index: -1;
    border-radius: inherit;
    background: radial-gradient(140px 60px at var(--mx) 100%, color-mix(in srgb, var(--accent) 22%, transparent), transparent 70%);
    pointer-events: none;
  }
  @keyframes spin {
    to {
      --spin: 360deg;
    }
  }

  /* ---- logo / start ---- */
  .logo {
    position: relative;
    width: 38px;
    height: 38px;
    margin-right: 2px;
    display: grid;
    place-items: center;
  }
  /* a new version is out: small accent dot on Start */
  .upd-dot {
    position: absolute;
    z-index: 1;
    top: 1px;
    right: 1px;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
    box-shadow:
      0 0 0 2px var(--bar-bg-solid),
      0 0 8px var(--accent);
  }
  .logo-glow {
    position: absolute;
    inset: 3px;
    border-radius: 12px;
    background: conic-gradient(from 200deg, var(--aurora-1), var(--aurora-2), var(--aurora-3), var(--aurora-1));
    filter: blur(8px);
    opacity: 0;
    transition: opacity 0.3s;
  }
  .logo-tile {
    position: relative;
    width: 36px;
    height: 36px;
    padding: 3px;
    border-radius: 11px;
    background:
      radial-gradient(circle at 30% 0%, color-mix(in srgb, var(--aurora-2) 45%, transparent), transparent 70%),
      linear-gradient(180deg, var(--tile-1), var(--tile-2));
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.18),
      inset 0 0 0 1px rgba(255, 255, 255, 0.08),
      0 2px 8px rgba(0, 0, 0, 0.4);
    transition: transform 0.3s var(--ease), border-radius 0.3s var(--ease);
  }
  .logo:hover .logo-glow {
    opacity: 0.8;
  }
  .logo:hover .logo-tile {
    transform: translateY(-1px);
    border-radius: 14px;
  }
  .logo:active .logo-tile {
    transform: scale(0.88);
  }

  .tool {
    width: 36px;
    height: 36px;
    padding: 9px;
    border-radius: 12px;
    color: var(--fg-dim);
    transition: background 0.15s, color 0.15s;
  }
  .tool:hover {
    background: rgba(255, 255, 255, 0.07);
    color: var(--fg);
  }
  /* virtual desktops: one dot each, the current one long and coloured */
  .desks {
    display: flex;
    align-items: center;
    gap: 4px;
    height: 26px;
    padding: 0 7px;
    margin: 0 2px;
    border-radius: 13px;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
  }
  .desk {
    width: 6px;
    height: 6px;
    border-radius: 3px;
    background: var(--fg-faint);
    transition:
      width 0.25s var(--ease),
      background 0.2s;
  }
  .desk:hover {
    background: var(--fg-dim);
  }
  .desk.on {
    width: 16px;
    background: linear-gradient(90deg, var(--aurora-1), var(--aurora-2));
  }
  .sep {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0 7px;
  }
  .sep i {
    width: 3px;
    height: 3px;
    border-radius: 50%;
    background: var(--fg-faint);
  }

  /* ---- dock apps ---- */
  .app {
    --m: 0;
    position: relative;
    height: 44px;
    min-width: 46px;
    padding: 0 7px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    border-radius: 14px;
    transition: background 0.2s;
  }
  .app:hover {
    background: rgba(255, 255, 255, 0.05);
  }
  .app.shifting {
    transition:
      transform 0.22s cubic-bezier(0.3, 0.9, 0.3, 1),
      background 0.2s;
  }
  .app.dragging {
    transition: none;
    cursor: grabbing;
    background: rgba(255, 255, 255, 0.08);
    box-shadow: 0 8px 20px -6px rgba(0, 0, 0, 0.6);
  }
  .lift {
    display: block;
    transform-origin: 50% 100%;
    transition: transform 0.18s var(--ease);
  }
  .app:global(.mag) .lift {
    transform: translateY(calc(var(--m) * -5px)) scale(calc(1 + var(--m) * 0.3));
  }
  .app:active .lift {
    transform: scale(0.85);
  }
  /* the focused app sits a little higher, with a deeper shadow */
  .app.active .lift {
    transform: translateY(-4px);
  }
  .app.active :global(.app-icon img) {
    filter: drop-shadow(0 5px 6px rgba(0, 0, 0, 0.5)) drop-shadow(0 0 10px color-mix(in srgb, var(--accent) 35%, transparent));
  }
  .app.active {
    padding: 0 4px; /* same button width with the bigger icon: neighbours don't move */
    background:
      radial-gradient(60% 80% at 50% 110%, color-mix(in srgb, var(--accent) 38%, transparent), transparent 70%),
      rgba(255, 255, 255, 0.06);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  /* a file dragged over this app: it will open here when dropped */
  .app.drop-target {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .app.drop-target .lift {
    transform: translateY(-5px) scale(1.12);
  }
  /* the app asks for attention (Teams message, download done...): warm glow
     breathing behind the icon + orange indicator, until it is opened */
  .app.attention {
    --warn: #ffa53d;
  }
  .app.attention::before {
    content: "";
    position: absolute;
    inset: 2px;
    z-index: -1;
    border-radius: 13px;
    background:
      radial-gradient(75% 75% at 50% 60%, color-mix(in srgb, var(--warn) 75%, transparent), transparent 75%),
      color-mix(in srgb, var(--warn) 14%, transparent);
    box-shadow:
      inset 0 0 0 1.5px color-mix(in srgb, var(--warn) 70%, transparent),
      0 0 14px color-mix(in srgb, var(--warn) 55%, transparent);
    animation: breathe 1.6s ease-in-out infinite;
  }
  .app.running.attention .ind {
    width: 16px;
    background: var(--warn);
    box-shadow: 0 0 10px var(--warn);
  }
  @keyframes breathe {
    0%,
    100% {
      opacity: 0.5;
      transform: scale(0.92);
    }
    50% {
      opacity: 1;
      transform: scale(1.05);
    }
  }
  /* macOS-like hop: quick rise, gravity fall, a little squash on landing */
  .app.launching .lift {
    animation: hop-mac 0.72s infinite;
  }
  @keyframes hop-mac {
    0% {
      transform: translateY(0);
      animation-timing-function: cubic-bezier(0.2, 0.7, 0.35, 1);
    }
    42% {
      transform: translateY(-11px);
      animation-timing-function: cubic-bezier(0.6, 0, 0.85, 0.45);
    }
    76% {
      transform: translateY(0) scale(1.07, 0.9);
      animation-timing-function: ease-out;
    }
    88% {
      transform: translateY(0) scale(0.98, 1.03);
    }
    100% {
      transform: translateY(0);
    }
  }
  .label {
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 12px;
  }
  .ind {
    position: absolute;
    bottom: 2px;
    left: 50%;
    width: 0;
    height: 3px;
    border-radius: 3px;
    transform: translateX(-50%);
    background: var(--fg-faint);
    transition: width 0.3s var(--ease), background 0.3s, box-shadow 0.3s;
  }
  .app.running .ind {
    width: 5px;
  }
  .app.running .ind.multi {
    width: 12px;
    background: linear-gradient(90deg, var(--fg-faint) 0 40%, transparent 40% 60%, var(--fg-faint) 60%);
  }
  .app.active .ind {
    width: 18px;
    background: linear-gradient(90deg, var(--aurora-1), var(--aurora-2));
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent) 80%, transparent);
  }

  .ripple {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 40px;
    height: 40px;
    margin: -20px 0 0 -20px;
    border-radius: 50%;
    border: 2px solid var(--accent);
    pointer-events: none;
    animation: ripple 0.8s cubic-bezier(0.2, 0.7, 0.3, 1) forwards;
  }
  @keyframes ripple {
    from {
      transform: scale(0.4);
      opacity: 0.9;
    }
    to {
      transform: scale(1.6);
      opacity: 0;
    }
  }
  .logo-tile :global(svg) {
    overflow: visible;
  }
  .logo-tile :global(.d-main),
  .logo-tile :global(.d-side),
  .logo-tile :global(.d-ind) {
    transform-box: fill-box;
    transform-origin: 50% 100%;
  }
  /* hover: the raised app hops and its indicator stretches */
  .logo:hover .logo-tile :global(.d-main) {
    animation: hop 0.7s cubic-bezier(0.3, 0.9, 0.3, 1.2);
  }
  .logo:hover .logo-tile :global(.d-ind) {
    animation: stretch 0.7s ease;
  }
  .logo:hover .logo-tile :global(.d-l) {
    animation: nudge-l 0.7s ease;
  }
  .logo:hover .logo-tile :global(.d-r) {
    animation: nudge-r 0.7s ease;
  }
  @keyframes hop {
    35% { transform: translateY(-2.5px) scale(1.08); }
    70% { transform: translateY(0.5px) scale(0.97); }
  }
  @keyframes stretch {
    40% { transform: scaleX(1.6); }
  }
  @keyframes nudge-l {
    40% { transform: translateX(-0.8px); }
  }
  @keyframes nudge-r {
    40% { transform: translateX(0.8px); }
  }

  /* ---- media ---- */
  .media {
    display: flex;
    align-items: center;
    gap: 1px;
    max-width: 320px;
  }
  .media-main {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 8px 4px 4px;
    border-radius: 13px;
    min-width: 0;
  }
  .media-main:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .disc {
    width: 32px;
    height: 32px;
    flex: none;
    border-radius: 50%;
    background:
      radial-gradient(circle, #0d0d14 0 3px, transparent 3.5px),
      repeating-radial-gradient(circle, rgba(255, 255, 255, 0.07) 0 1px, transparent 1px 3px),
      conic-gradient(var(--aurora-1), var(--aurora-3), var(--aurora-2), var(--aurora-1));
    box-shadow: 0 0 12px color-mix(in srgb, var(--aurora-2) 40%, transparent);
  }
  /* the song's cover, as a little vinyl (small hole in the middle) */
  .disc.cover {
    background-size: cover;
    background-position: center;
    box-shadow:
      0 0 0 1px rgba(255, 255, 255, 0.12),
      0 2px 8px rgba(0, 0, 0, 0.45);
    position: relative;
  }
  .disc.cover i {
    position: absolute;
    left: 50%;
    top: 50%;
    width: 7px;
    height: 7px;
    margin: -3.5px 0 0 -3.5px;
    border-radius: 50%;
    background: var(--surface-2);
    box-shadow: 0 0 0 1.5px rgba(255, 255, 255, 0.25);
  }
  /* spins only while hovered (a forever-spinning disc costs GPU frames) */
  .disc.spin {
    animation: rot 3s linear infinite paused;
  }
  .media:hover .disc.spin {
    animation-play-state: running;
  }
  @keyframes rot {
    to { transform: rotate(360deg); }
  }
  .media-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.2;
    text-align: left;
  }
  .media-text b,
  .media-text small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: 140px;
  }
  .media-text b {
    font-size: 12px;
    font-weight: 600;
  }
  .media-text small {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .mc {
    width: 28px;
    height: 28px;
    padding: 7px;
    flex: none;
  }
  .mc.play {
    width: 30px;
    height: 30px;
    padding: 8px;
    border-radius: 50%;
    background: linear-gradient(135deg, #fff, #dcd8ff);
    color: #111;
    margin: 0 2px;
  }

  /* ---- gauges ---- */
  .stats {
    display: flex;
    gap: 4px;
    padding: 0 4px;
    height: 40px;
    align-items: center;
    border-radius: 13px;
  }
  .stats:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .media + .stats {
    margin-left: 4px;
    padding-left: 10px;
    border-left: 1px solid var(--stroke);
    border-radius: 0 13px 13px 0;
  }
  .gauge {
    position: relative;
    width: 36px;
    height: 36px;
  }
  .gauge svg {
    transform: rotate(-90deg);
  }
  .gauge circle {
    fill: none;
    stroke-width: 3.2;
  }
  .track {
    stroke: rgba(255, 255, 255, 0.08);
  }
  .val {
    stroke: url(#gaugeGrad);
    stroke-linecap: round;
    transition: stroke-dasharray 0.25s ease;
    filter: drop-shadow(0 0 3px color-mix(in srgb, var(--aurora-2) 70%, transparent));
  }
  .gauge.hot .val {
    stroke: #ff5f6d;
  }
  .gl {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .gl b {
    font-size: 11px;
    font-weight: 700;
  }
  .gl small {
    font-size: 7px;
    letter-spacing: 0.08em;
    color: var(--fg-dim);
  }

  /* ---- system island ---- */
  .chip {
    height: 36px;
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 0 12px;
    border-radius: 12px;
    background: rgba(255, 255, 255, 0.05);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    transition: background 0.15s;
  }
  .chip:hover {
    background: rgba(255, 255, 255, 0.09);
  }
  .chip .g {
    width: 17px;
    height: 17px;
  }
  .chip small {
    font-size: 11px;
    color: var(--fg-dim);
    margin-left: -4px;
  }
  .clock {
    height: 40px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    justify-content: center;
    padding: 0 10px;
    border-radius: 12px;
    line-height: 1.1;
  }
  .clock:hover {
    background: rgba(255, 255, 255, 0.06);
  }
  .time {
    display: grid;
    overflow: hidden;
    height: 20px;
  }
  .time b {
    grid-area: 1 / 1;
    text-align: right;
  }
  .vol {
    position: relative;
  }
  .vbar {
    position: absolute;
    left: -6px;
    right: -6px;
    bottom: -7px;
    height: 3px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.12);
    overflow: hidden;
  }
  .vbar i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, var(--aurora-1), var(--aurora-2));
    transition: width 0.15s;
  }
  .clock b {
    font-family: "Segoe UI Variable Display", "Segoe UI", sans-serif;
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.01em;
    font-variant-numeric: tabular-nums;
    background: linear-gradient(180deg, #fff 30%, color-mix(in srgb, var(--aurora-2) 45%, #fff));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }
  .clock small {
    font-size: 10.5px;
    color: var(--fg-dim);
    text-transform: capitalize;
  }
  :global(:root[data-scheme="light"]) .clock b {
    background: none;
    color: var(--fg);
  }
  .peek {
    width: 6px;
    height: 26px;
    margin: 0 2px 0 4px;
    border-radius: 3px;
    background: var(--stroke);
    transition: background 0.2s, height 0.2s var(--ease);
  }
  .peek:hover {
    height: 34px;
    background: linear-gradient(180deg, var(--aurora-1), var(--aurora-2));
  }
</style>
