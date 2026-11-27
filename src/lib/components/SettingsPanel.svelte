<script lang="ts">
  import { api, type BackupInfo, type Settings } from "$lib/api";
  import { ico } from "$lib/icons";
  import { onMount } from "svelte";
  import { checkNow, initUpdate, installNow, update } from "$lib/update.svelte";
  import { ACCENTS, THEMES, loadWallpaperTheme, themeById, type Theme } from "$lib/theme";

  let { settings, onBack }: { settings: Settings; onBack?: () => void } = $props();

  type BoolKey =
    | "centered"
    | "showMedia"
    | "showStats"
    | "showLabels"
    | "allMonitors"
    | "autostart"
    | "autoHide"
    | "floating"
    | "clockSeconds"
    | "reduceMotion";

  // ---- appearance ----
  const iconSizes = [
    { v: "small", label: "Piccole" },
    { v: "medium", label: "Medie" },
    { v: "large", label: "Grandi" },
  ] as const;
  const corners = [
    { v: "square", label: "Squadrati" },
    { v: "medium", label: "Arrotondati" },
    { v: "round", label: "Pillola" },
  ] as const;
  const autoModes = [
    { v: "off", label: "No" },
    { v: "windows", label: "Come Windows" },
    { v: "time", label: "Giorno/notte" },
  ] as const;
  function setChoice<K extends "iconSize" | "roundness" | "autoTheme">(key: K, v: Settings[K]) {
    const s = $state.snapshot(settings);
    s[key] = v;
    api.saveSettings(s);
  }
  const options: { key: BoolKey; label: string; hint: string }[] = [
    { key: "autostart", label: "Avvia con Windows", hint: "Parte da sola all'accesso" },
    { key: "floating", label: "Barra flottante", hint: "Isole sopra le finestre invece della barra fissa a tutta larghezza" },
    { key: "autoHide", label: "Nascondi automaticamente", hint: "Solo barra flottante: ricompare in fondo allo schermo" },
    { key: "centered", label: "Icone centrate", hint: "Altrimenti allineate a sinistra" },
    { key: "showMedia", label: "Widget musica", hint: "Brano in riproduzione e controlli" },
    { key: "showStats", label: "CPU e RAM", hint: "Mini indicatori a sinistra" },
    { key: "showLabels", label: "Titoli finestre", hint: "Mostra il titolo accanto all'icona" },
    { key: "allMonitors", label: "Tutte le finestre su ogni schermo", hint: "Altrimenti ogni barra mostra solo le sue" },
  ];

  const looks: { key: BoolKey; label: string; hint: string }[] = [
    { key: "clockSeconds", label: "Secondi nell'orologio", hint: "Mostra anche i secondi" },
    { key: "reduceMotion", label: "Riduci animazioni", hint: "Niente rimbalzi, rotazioni e transizioni" },
  ];

  function toggle(key: BoolKey) {
    const s = $state.snapshot(settings);
    s[key] = !s[key];
    api.saveSettings(s);
  }

  // ---- backup ----
  let backups = $state<BackupInfo[] | null>(null);
  let note = $state("");

  async function toggleBackups() {
    backups = backups ? null : await api.listBackups();
  }

  const when = (t: number) =>
    new Date(t * 1000).toLocaleString("it-IT", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  async function restore(b: BackupInfo) {
    try {
      const s = await api.readBackup(b.id);
      await api.saveSettings(s);
      note = `Ripristinate le impostazioni di ${when(b.time)}`;
      backups = null;
    } catch (e) {
      note = `Ripristino non riuscito: ${e}`;
    }
  }

  async function doExport() {
    note = "";
    try {
      await api.exportSettings(); // the panel closes while the dialog is open
    } catch (e) {
      note = `Esportazione non riuscita: ${e}`;
    }
  }

  async function doImport() {
    note = "";
    try {
      const s = await api.importSettings();
      if (s) await api.saveSettings(s);
    } catch (e) {
      note = `Importazione non riuscita: ${e}`;
    }
  }

  // ---- theme ----
  function setTheme(id: string) {
    const s = $state.snapshot(settings);
    s.theme = id;
    // a custom accent survives a theme change; "Windows" too
    api.saveSettings(s);
  }

  function setAccent(a: string) {
    const s = $state.snapshot(settings);
    s.accent = a;
    api.saveSettings(s);
  }

  // the "Dallo sfondo" card shows the wallpaper's real colours
  let wallTheme = $state<Theme | null>(null);
  onMount(() => {
    initUpdate();
    loadWallpaperTheme().then((t) => (wallTheme = t));
  });
  const allThemes = $derived(wallTheme ? [...THEMES, wallTheme] : THEMES);

  const theme = $derived(settings.theme === "sfondo" && wallTheme ? wallTheme : themeById(settings.theme));
  const accentMode = $derived(settings.accent ?? "system");
  const customAccent = $derived(
    /^#[0-9a-f]{6}$/i.test(accentMode) && !ACCENTS.includes(accentMode.toLowerCase()) ? accentMode : null,
  );
  // the colour picker fires continuously while dragging: save at most every 120ms
  let pickTimer = 0;
  function pickColor(e: Event) {
    const v = (e.currentTarget as HTMLInputElement).value;
    document.documentElement.style.setProperty("--accent", v); // instant feedback
    clearTimeout(pickTimer);
    pickTimer = setTimeout(() => setAccent(v), 120) as unknown as number;
  }
</script>

<div class="settings">
  <h3>
    {#if onBack}
      <button class="back" title="Torna a Start" aria-label="Torna a Start" onclick={onBack}>{@html ico.chevL}</button>
    {/if}
    Impostazioni barra
  </h3>
  <div class="list">
    <section class="themes">
      <h4>Tema</h4>
      <div class="theme-grid">
        {#each allThemes as t (t.id)}
          <button
            class="theme"
            class:sel={theme.id === t.id}
            onclick={() => setTheme(t.id)}
            title="Tema {t.name}"
            style="--s1:{t.surface[0]};--s2:{t.surface[1]};--st1:{t.strip[0]};--st2:{t.strip[1]};--c1:{t.accent};--c2:{t.a2};--c3:{t.a3}"
          >
            <span class="mini" class:light={t.scheme === "light"}>
              <span class="mini-dock"><i></i><i></i><i></i></span>
            </span>
            <span class="tname">{t.name}</span>
          </button>
        {/each}
      </div>

      <div class="seg-row">
        <span class="seg-label">Tema chiaro automatico</span>
        <div class="seg">
          {#each autoModes as o (o.v)}
            <button class:on={(settings.autoTheme ?? "off") === o.v} onclick={() => setChoice("autoTheme", o.v)}>{o.label}</button>
          {/each}
        </div>
      </div>
      {#if (settings.autoTheme ?? "off") !== "off"}
        <p class="auto-note">
          {settings.autoTheme === "windows"
            ? "Usa il tema Chiaro quando Windows è in modalità chiara, il tema scelto sopra quando è scura."
            : "Tema Chiaro dalle 7:00 alle 19:00, il tema scelto sopra la sera e la notte."}
        </p>
      {/if}

      <h4>Colore d'accento</h4>
      <div class="accents">
        <button class="sw sys" class:sel={accentMode === "system"} onclick={() => setAccent("system")} title="Come Windows">
          <span>Win</span>
        </button>
        <button
          class="sw"
          class:sel={accentMode === ""}
          style="--c:{theme.accent}"
          onclick={() => setAccent("")}
          title="Colore del tema {theme.name}"
        ><span class="dot">T</span></button>
        {#each ACCENTS as c (c)}
          <button class="sw" class:sel={accentMode.toLowerCase() === c} style="--c:{c}" onclick={() => setAccent(c)} title={c}></button>
        {/each}
        <label class="sw pick" class:sel={!!customAccent} title="Scegli un colore" style={customAccent ? `--c:${customAccent}` : ""}>
          <input type="color" value={customAccent ?? theme.accent} oninput={pickColor} />
          {#if !customAccent}<span>+</span>{/if}
        </label>
      </div>
    </section>

    <h4>Aspetto</h4>
    <div class="seg-row">
      <span class="seg-label">Dimensione icone</span>
      <div class="seg">
        {#each iconSizes as o (o.v)}
          <button class:on={(settings.iconSize ?? "medium") === o.v} onclick={() => setChoice("iconSize", o.v)}>{o.label}</button>
        {/each}
      </div>
    </div>
    <div class="seg-row">
      <span class="seg-label">Angoli delle isole</span>
      <div class="seg">
        {#each corners as o (o.v)}
          <button class:on={(settings.roundness ?? "medium") === o.v} onclick={() => setChoice("roundness", o.v)}>{o.label}</button>
        {/each}
      </div>
    </div>
    {#each looks as o}
      <button class="opt" onclick={() => toggle(o.key)}>
        <span class="txt"><b>{o.label}</b><small>{o.hint}</small></span>
        <span class="switch" class:on={settings[o.key]}><i></i></span>
      </button>
    {/each}

    <h4>Barra</h4>
    {#each options.filter((o) => o.key !== "autoHide" || settings.floating) as o}
      <button class="opt" onclick={() => toggle(o.key)}>
        <span class="txt"><b>{o.label}</b><small>{o.hint}</small></span>
        <span class="switch" class:on={settings[o.key]}><i></i></span>
      </button>
    {/each}

    <h4>Aggiornamenti</h4>
    {#if update.available}
      <div class="upd">
        <div class="upd-head">
          <span class="upd-dot"></span>
          <span class="txt">
            <b>Nuova versione {update.available.version}</b>
            <small>Ora hai la {update.current}</small>
          </span>
          <button class="chip-btn accent" disabled={update.installing} onclick={installNow}>
            {update.installing ? "Scarico…" : "Aggiorna ora"}
          </button>
        </div>
        {#if update.available.notes}<p class="upd-notes">{update.available.notes}</p>{/if}
        {#if update.installing}
          <div class="upd-bar" class:indet={update.progress === null}>
            <i style="width:{(update.progress ?? 0.3) * 100}%"></i>
          </div>
        {/if}
      </div>
    {:else if update.store}
      <div class="seg-row">
        <span class="txt">
          <b>Iacobar {update.current}</b>
          <small>Gli aggiornamenti arrivano dal Microsoft Store</small>
        </span>
      </div>
    {:else}
      <div class="seg-row">
        <span class="txt">
          <b>Iacobar {update.current}</b>
          <small>{update.upToDate ? "È l'ultima versione" : "Si aggiorna da sola quando esce una nuova versione"}</small>
        </span>
        <button class="chip-btn" disabled={update.checking} onclick={checkNow}>
          {update.checking ? "Controllo…" : "Cerca aggiornamenti"}
        </button>
      </div>
    {/if}
    {#if update.error}<p class="note">{update.error}</p>{/if}

    <h4>Backup</h4>
    <div class="backup">
      <button class="chip-btn" onclick={doExport} title="Salva app fissate, tema e opzioni in un file">Esporta…</button>
      <button class="chip-btn" onclick={doImport} title="Carica le impostazioni da un file esportato">Importa…</button>
      <button class="chip-btn" class:on={!!backups} onclick={toggleBackups} title="Copie salvate in automatico prima di ogni modifica">
        Copie automatiche
      </button>
    </div>
    {#if backups}
      <ul class="copies">
        {#each backups as b (b.id)}
          <li>
            <span class="when">{when(b.time)}</span>
            <small>{b.pinned} app fissate</small>
            <button class="restore" onclick={() => restore(b)}>Ripristina</button>
          </li>
        {:else}
          <li class="empty">Nessuna copia ancora: la prima viene salvata alla prossima modifica.</li>
        {/each}
      </ul>
    {/if}
    {#if note}<p class="note">{note}</p>{/if}
  </div>

  <div class="buttons">
    <button class="btn" onclick={() => api.shell("taskmgr")}><span class="bi">{@html ico.taskmgr}</span>Gestione attività</button>
    <button class="btn" onclick={() => api.shell("settings")}><span class="bi">{@html ico.gear}</span>Impostazioni di Windows</button>
    <button class="btn quit" onclick={() => api.quit()}>
      <span class="bi">{@html ico.power}</span>Esci e ripristina la barra di Windows
    </button>
  </div>
</div>

<style>
  .settings {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 18px 14px 14px;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 6px 12px;
    font-size: 16px;
    font-weight: 600;
  }
  .back {
    width: 28px;
    height: 28px;
    margin-left: -6px;
    padding: 6px;
    border-radius: 8px;
    color: var(--fg-dim);
  }
  .back:hover {
    background: var(--hover);
    color: var(--fg);
  }
  /* never cut off: the options scroll if the window is short */
  .list {
    min-height: 0;
    flex: 1 1 auto;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  h4 {
    margin: 10px 10px 8px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-dim);
  }
  .themes h4:first-child {
    margin-top: 0;
  }
  /* ---- theme cards: a tiny bar drawn with the theme's colours ---- */
  .theme-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
    gap: 8px;
    padding: 0 6px;
  }
  .theme {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    border-radius: 12px;
    transition: background 0.15s, box-shadow 0.15s;
  }
  .theme:hover {
    background: var(--hover);
  }
  .theme.sel {
    box-shadow: inset 0 0 0 2px var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .mini {
    position: relative;
    height: 46px;
    border-radius: 8px;
    overflow: hidden;
    background:
      radial-gradient(60px 40px at 15% 0%, color-mix(in srgb, var(--c1) 55%, transparent), transparent 70%),
      radial-gradient(60px 40px at 90% 10%, color-mix(in srgb, var(--c3) 45%, transparent), transparent 70%),
      linear-gradient(160deg, #2a3350, #1a1530);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
  }
  .mini.light {
    background:
      radial-gradient(60px 40px at 15% 0%, color-mix(in srgb, var(--c1) 35%, transparent), transparent 70%),
      linear-gradient(160deg, #dfe6f5, #f1e8f6);
  }
  .mini::after {
    /* the bar strip */
    content: "";
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 15px;
    background: linear-gradient(180deg, var(--st1), var(--st2));
  }
  .mini-dock {
    position: absolute;
    z-index: 1;
    left: 50%;
    bottom: 3px;
    transform: translateX(-50%);
    display: flex;
    gap: 3px;
    padding: 2px 5px;
    border-radius: 5px;
    background: linear-gradient(180deg, var(--s1), var(--s2));
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--c2) 45%, transparent);
  }
  .mini-dock i {
    width: 6px;
    height: 6px;
    border-radius: 2px;
    background: var(--c1);
  }
  .mini-dock i:nth-child(2) {
    background: var(--c2);
  }
  .mini-dock i:nth-child(3) {
    background: var(--c3);
  }
  .tname {
    font-size: 12px;
    text-align: center;
  }
  /* ---- accent swatches ---- */
  .accents {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 0 10px 6px;
  }
  .sw {
    position: relative;
    width: 26px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--c, var(--card));
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.15);
    font-size: 9px;
    font-weight: 700;
    color: rgba(0, 0, 0, 0.7);
    transition: transform 0.15s var(--ease), box-shadow 0.15s;
  }
  .sw:hover {
    transform: scale(1.12);
  }
  .sw.sel {
    box-shadow:
      0 0 0 2px var(--bar-bg-solid),
      0 0 0 4px var(--fg);
  }
  .sw.sys {
    width: auto;
    padding: 0 8px;
    border-radius: 13px;
    color: var(--fg);
    background: var(--card);
    box-shadow: inset 0 0 0 1px var(--stroke);
  }
  .sw.sys.sel {
    box-shadow:
      inset 0 0 0 1px var(--stroke),
      0 0 0 2px var(--bar-bg-solid),
      0 0 0 4px var(--fg);
  }
  .sw.pick {
    overflow: hidden;
    color: var(--fg);
    font-size: 15px;
    font-weight: 400;
    background: var(--c, conic-gradient(#f87171, #facc15, #4ade80, #2dd4bf, #4cc2ff, #c084fc, #f472b6, #f87171));
  }
  .sw.pick span {
    position: relative;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.6);
    color: #fff;
    pointer-events: none;
  }
  .sw.pick input {
    position: absolute;
    inset: 0;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: default;
  }
  /* ---- appearance: segmented choices ---- */
  .seg-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 10px;
  }
  .seg-label {
    font-weight: 600;
  }
  .seg {
    display: flex;
    padding: 3px;
    border-radius: 10px;
    background: var(--card);
    box-shadow: inset 0 0 0 1px var(--stroke);
  }
  .seg button {
    height: 26px;
    padding: 0 11px;
    border-radius: 7px;
    font-size: 12px;
    color: var(--fg-dim);
    transition: background 0.15s, color 0.15s;
  }
  .seg button:hover {
    color: var(--fg);
  }
  .seg button.on {
    background: var(--accent);
    color: #0b0b12;
    font-weight: 600;
  }
  .auto-note {
    margin: 0 12px 6px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--fg-dim);
  }
  /* ---- backup ---- */
  .backup {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 0 10px 8px;
  }
  .chip-btn {
    height: 30px;
    padding: 0 12px;
    border-radius: 15px;
    background: var(--card);
    box-shadow: inset 0 0 0 1px var(--stroke);
    font-size: 12.5px;
    transition: background 0.15s;
  }
  .chip-btn:hover {
    background: var(--hover);
  }
  .chip-btn.on {
    background: color-mix(in srgb, var(--accent) 18%, var(--card));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 50%, transparent);
  }
  .chip-btn:disabled {
    opacity: 0.6;
  }
  .chip-btn.accent {
    flex: none;
    background: var(--accent);
    color: #0b0b12;
    font-weight: 600;
    box-shadow: none;
  }
  .chip-btn.accent:hover {
    background: color-mix(in srgb, var(--accent) 85%, #fff);
  }
  /* ---- updates ---- */
  .upd {
    margin: 0 10px 8px;
    padding: 10px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 12%, var(--card));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .upd-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .upd-dot {
    width: 8px;
    height: 8px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
  }
  .upd-notes {
    margin: 8px 0 0 18px;
    font-size: 12px;
    line-height: 1.4;
    color: var(--fg-dim);
    white-space: pre-line;
  }
  .upd-bar {
    margin: 10px 0 2px 18px;
    height: 4px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--stroke);
  }
  .upd-bar i {
    display: block;
    height: 100%;
    border-radius: 2px;
    background: var(--accent);
    transition: width 0.2s;
  }
  .upd-bar.indet i {
    animation: upd-slide 1.1s ease-in-out infinite;
  }
  @keyframes upd-slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .copies {
    list-style: none;
    margin: 0 10px 8px;
    padding: 4px;
    border-radius: 10px;
    background: var(--card);
    border: 1px solid var(--stroke);
  }
  .copies li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
    border-radius: 7px;
  }
  .copies li:hover {
    background: var(--hover);
  }
  .when {
    flex: 1;
    text-transform: capitalize;
  }
  .copies small {
    color: var(--fg-dim);
    font-size: 11px;
  }
  .restore {
    padding: 3px 10px;
    border-radius: 6px;
    font-size: 12px;
    color: var(--accent);
  }
  .restore:hover {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
  }
  .copies li.empty {
    color: var(--fg-dim);
    font-size: 12px;
  }
  .note {
    margin: 0 12px 8px;
    font-size: 12px;
    color: var(--fg-dim);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px;
    border-radius: 8px;
    text-align: left;
  }
  .opt:hover {
    background: var(--hover);
  }
  .txt {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .txt small {
    color: var(--fg-faint);
    font-size: 11px;
  }
  .switch {
    width: 40px;
    height: 20px;
    border-radius: 10px;
    border: 1px solid var(--fg-dim);
    position: relative;
    transition: background 0.2s, border-color 0.2s;
    flex: none;
  }
  .switch i {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--fg-dim);
    transition: transform 0.25s var(--ease), background 0.2s;
  }
  .switch.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .switch.on i {
    transform: translateX(20px);
    background: #000;
  }
  .buttons {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-top: 1px solid var(--stroke);
    padding-top: 8px;
  }
  .btn {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 10px;
    border-radius: 8px;
  }
  .btn:hover {
    background: var(--hover);
  }
  .btn.quit:hover {
    color: #ff6b6b;
  }
  .bi {
    width: 16px;
    height: 16px;
  }
</style>
