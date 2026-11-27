<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, type AppEntry, type Settings } from "$lib/api";
  import { ico } from "$lib/icons";
  import AppIcon from "./AppIcon.svelte";

  let { settings, focusSearch = false, onSettings }: {
    settings: Settings;
    focusSearch?: boolean;
    onSettings: () => void;
  } = $props();

  let apps = $state<AppEntry[]>([]);
  let query = $state("");
  let sel = $state(0);
  let input: HTMLInputElement;
  let powerOpen = $state(false);
  let confirm = $state<string | null>(null);
  let copied = $state(false);

  onMount(async () => {
    apps = await api.listApps();
    await tick();
    input?.focus();
  });

  // ---- search -------------------------------------------------------------

  function score(name: string, q: string): number {
    const n = name.toLowerCase();
    if (n.startsWith(q)) return 3;
    if (n.split(/[\s\-_.]/).some((w) => w.startsWith(q))) return 2;
    if (n.includes(q)) return 1;
    // initials: "vsc" -> "Visual Studio Code"
    const initials = n.split(/\s+/).map((w) => w[0]).join("");
    return initials.startsWith(q) ? 1 : 0;
  }

  const results = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return apps
      .map((a) => ({ a, s: score(a.name, q) }))
      .filter((x) => x.s > 0)
      .sort((x, y) => y.s - x.s || x.a.name.localeCompare(y.a.name))
      .slice(0, 8)
      .map((x) => x.a);
  });

  const math = $derived.by(() => {
    const q = query.trim().replace(/,/g, ".").replace(/\^/g, "**").replace(/x/gi, "*");
    if (!/^[\d\s+\-*/().%]+$/.test(q) || !/[+\-*/%]/.test(q)) return null;
    try {
      const v = Function(`"use strict"; return (${q})`)();
      return typeof v === "number" && isFinite(v) ? +v.toPrecision(12) : null;
    } catch {
      return null;
    }
  });

  // rows = apps + (math) + web search
  const rowCount = $derived(results.length + (math !== null ? 1 : 0) + (query.trim() ? 1 : 0));

  $effect(() => {
    query;
    sel = 0;
  });

  function open(a: AppEntry) {
    api.launch(a.target);
  }

  function webSearch() {
    api.launch(`https://www.bing.com/search?q=${encodeURIComponent(query.trim())}`);
  }

  async function copyMath() {
    if (math === null) return;
    await navigator.clipboard.writeText(String(math));
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }

  function activateRow(i: number) {
    if (i < results.length) return open(results[i]);
    i -= results.length;
    if (math !== null) {
      if (i === 0) return copyMath();
      i--;
    }
    webSearch();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      sel = Math.min(sel + 1, rowCount - 1);
      e.preventDefault();
    } else if (e.key === "ArrowUp") {
      sel = Math.max(sel - 1, 0);
      e.preventDefault();
    } else if (e.key === "Enter" && rowCount > 0) {
      activateRow(sel);
    }
  }

  // ---- pinning ------------------------------------------------------------

  const pinnedIds = $derived(new Set(settings.pinned.map((p) => p.target.toLowerCase())));

  async function togglePin(a: AppEntry, e: Event) {
    e.preventDefault();
    e.stopPropagation();
    const s = $state.snapshot(settings);
    const idx = s.pinned.findIndex((p) => p.target.toLowerCase() === a.target.toLowerCase());
    if (idx >= 0) {
      s.pinned.splice(idx, 1);
    } else {
      const exe = a.target.toLowerCase().endsWith(".lnk") ? await api.resolveShortcut(a.target) : null;
      s.pinned.push({ name: a.name, target: a.target, appId: (exe ?? a.target).toLowerCase() });
    }
    await api.saveSettings(s);
  }

  // ---- power --------------------------------------------------------------

  const power = [
    { id: "lock", label: "Blocca", icon: ico.lock, run: () => api.launch("rundll32.exe", "user32.dll,LockWorkStation") },
    { id: "sleep", label: "Sospendi", icon: ico.moon, run: () => api.launch("rundll32.exe", "powrprof.dll,SetSuspendState 0,1,0") },
    { id: "logout", label: "Disconnetti", icon: ico.logout, run: () => api.launch("shutdown.exe", "/l"), ask: true },
    { id: "restart", label: "Riavvia", icon: ico.restart, run: () => api.launch("shutdown.exe", "/r /t 0"), ask: true },
    { id: "shutdown", label: "Arresta", icon: ico.power, run: () => api.launch("shutdown.exe", "/s /t 0"), ask: true },
  ];

  function doPower(p: (typeof power)[number]) {
    if (p.ask && confirm !== p.id) {
      confirm = p.id;
      return;
    }
    p.run();
  }

  const hour = new Date().getHours();
  const greeting = hour < 12 ? "Buongiorno" : hour < 18 ? "Buon pomeriggio" : "Buonasera";
</script>

<div class="start">
  <div class="search">
    <span class="si">{@html ico.search}</span>
    <input
      bind:this={input}
      bind:value={query}
      onkeydown={onKey}
      placeholder="Cerca app, calcola, cerca sul web…"
      spellcheck="false"
      autocomplete="off"
    />
    {#if focusSearch}<span class="kbd">Invio</span>{/if}
  </div>

  <div class="body">
    {#if query.trim()}
      <ul class="results">
        {#each results as a, i (a.target)}
          <li>
            <button class="row" class:sel={sel === i} onmouseenter={() => (sel = i)} onclick={() => open(a)} oncontextmenu={(e) => togglePin(a, e)}>
              <AppIcon path={a.iconPath} name={a.name} size={28} />
              <span class="grow">{a.name}</span>
              <span class="hint">{pinnedIds.has(a.target.toLowerCase()) ? "Fissata" : "App"}</span>
            </button>
          </li>
        {/each}
        {#if math !== null}
          <li>
            <button class="row" class:sel={sel === results.length} onmouseenter={() => (sel = results.length)} onclick={copyMath}>
              <span class="ri">{@html ico.calc}</span>
              <span class="grow"><b class="big">= {math.toLocaleString("it-IT")}</b></span>
              <span class="hint">{copied ? "Copiato!" : "Invio per copiare"}</span>
            </button>
          </li>
        {/if}
        <li>
          <button class="row" class:sel={sel === rowCount - 1} onmouseenter={() => (sel = rowCount - 1)} onclick={webSearch}>
            <span class="ri">{@html ico.globe}</span>
            <span class="grow">Cerca «{query.trim()}» sul web</span>
            <span class="hint">Bing</span>
          </button>
        </li>
      </ul>
    {:else}
      <h3>{greeting} 👋</h3>
      <div class="section-title">
        <span>Fissate sulla barra</span>
        <small>Clic destro su un'app per fissarla</small>
      </div>
      <div class="grid">
        {#each settings.pinned as p (p.target)}
          <button class="tile" onclick={() => api.launch(p.target)} title={p.name}>
            <AppIcon path={p.appId || p.target} name={p.name} size={34} />
            <span>{p.name}</span>
          </button>
        {/each}
      </div>

      <div class="section-title"><span>Tutte le app</span><small>{apps.length}</small></div>
      <ul class="all">
        {#each apps as a (a.target)}
          <li>
            <button class="row compact" onclick={() => open(a)} oncontextmenu={(e) => togglePin(a, e)}>
              <AppIcon path={a.iconPath} name={a.name} size={22} />
              <span class="grow">{a.name}</span>
              <span class="pin" class:on={pinnedIds.has(a.target.toLowerCase())} title="Fissa / sblocca">
                {@html ico.pin}
              </span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <footer>
    <div class="left">
      <button class="icon-btn fbtn" title="Start di Windows" onclick={() => api.shell("start")}>{@html ico.windows}</button>
      <button class="icon-btn fbtn" title="Impostazioni della barra" onclick={onSettings}>{@html ico.sliders}</button>
      <button class="icon-btn fbtn" title="Impostazioni di Windows" onclick={() => api.shell("settings")}>{@html ico.gear}</button>
    </div>
    <div class="power" class:open={powerOpen}>
      {#if powerOpen}
        {#each power as p (p.id)}
          <button class="pbtn" class:warn={confirm === p.id} onclick={() => doPower(p)} title={p.label}>
            <span class="pi">{@html p.icon}</span>
            <span>{confirm === p.id ? "Conferma" : p.label}</span>
          </button>
        {/each}
      {/if}
      <button class="icon-btn fbtn" title="Alimentazione" onclick={() => { powerOpen = !powerOpen; confirm = null; }}>
        {@html powerOpen ? ico.close : ico.power}
      </button>
    </div>
  </footer>
</div>

<style>
  .start {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .search {
    margin: 18px 20px 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 14px;
    border-radius: 21px;
    background: var(--card);
    border: 1px solid var(--stroke);
    box-shadow:
      inset 0 -2px 0 -0.5px transparent,
      0 6px 20px -10px color-mix(in srgb, var(--aurora-2) 60%, transparent);
    background:
      linear-gradient(#171824, #171824) padding-box,
      linear-gradient(90deg, var(--accent), var(--aurora-2), var(--aurora-3)) border-box;
    border-color: transparent;
  }
  .si {
    width: 18px;
    height: 18px;
    color: var(--fg-dim);
  }
  input {
    flex: 1;
    border: 0;
    outline: 0;
    background: none;
    font: inherit;
    font-size: 14px;
    color: var(--fg);
  }
  input::placeholder {
    color: var(--fg-faint);
  }
  .kbd {
    font-size: 11px;
    color: var(--fg-faint);
  }
  .body {
    flex: 1;
    overflow-y: auto;
    padding: 4px 20px 12px;
  }
  h3 {
    font-weight: 600;
    font-size: 20px;
    margin: 14px 4px 6px;
  }
  .section-title {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin: 16px 4px 8px;
    font-weight: 600;
  }
  .section-title small {
    font-weight: 400;
    color: var(--fg-faint);
    font-size: 11px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 4px;
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 4px 10px;
    border-radius: var(--radius);
    font-size: 11.5px;
    transition: background 0.15s, transform 0.2s var(--ease);
  }
  .tile span {
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tile:hover {
    background: var(--hover);
    transform: translateY(-2px);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 8px;
    text-align: left;
  }
  .row.compact {
    padding: 6px 10px;
  }
  .row:hover,
  .row.sel {
    background: var(--hover);
  }
  .row.sel {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .grow {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .big {
    font-size: 16px;
  }
  .hint {
    font-size: 11px;
    color: var(--fg-faint);
  }
  .ri {
    width: 28px;
    height: 28px;
    padding: 4px;
    color: var(--accent);
  }
  .pin {
    width: 16px;
    height: 16px;
    opacity: 0;
    color: var(--fg-dim);
    transition: opacity 0.15s;
  }
  .row:hover .pin {
    opacity: 1;
  }
  .pin.on {
    opacity: 1;
    color: var(--accent);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    border-top: 1px solid var(--stroke);
    background: var(--press);
  }
  .left,
  .power {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .fbtn {
    width: 36px;
    height: 36px;
    padding: 9px;
  }
  .pbtn {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 3px;
    width: 64px;
    padding: 6px 2px;
    border-radius: 8px;
    font-size: 10.5px;
    animation: slide 0.25s var(--ease) both;
  }
  .pbtn:hover {
    background: var(--hover);
  }
  .pbtn.warn {
    background: color-mix(in srgb, #ff4d4f 25%, transparent);
  }
  .pi {
    width: 18px;
    height: 18px;
  }
  @keyframes slide {
    from {
      opacity: 0;
      transform: translateX(12px);
    }
  }
</style>
