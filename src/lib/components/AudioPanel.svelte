<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AppVolume, type AudioOut, type MediaInfo, type Radios, type SystemStatus, type WifiList, type WifiNetwork } from "$lib/api";
  import AppIcon from "./AppIcon.svelte";
  import { batteryIcon, ico, volumeIcon } from "$lib/icons";

  let status = $state<SystemStatus | null>(null);
  let media = $state<MediaInfo | null>(null);
  let art = $state<string | null>(null);
  let artKey = "";
  let dragging = false;
  let radios = $state<Radios | null>(null);
  let busy = $state<string | null>(null);
  let error = $state("");

  onMount(() => {
    const load = async () => {
      const s = await api.systemStatus();
      if (!dragging) status = s;
      media = await api.mediaInfo();
      const key = media ? `${media.title}|${media.artist}` : "";
      if (key !== artKey) {
        artKey = key;
        art = key ? await api.mediaArt().catch(() => null) : null;
      }
      if (!busy) radios = await api.radioStatus();
      if (draggingApp === null) apps = await api.appVolumes().catch(() => apps);
    };
    load();
    const t = setInterval(load, 1500);
    return () => clearInterval(t);
  });

  // ---- per-app volume ----
  let apps = $state<AppVolume[]>([]);
  let draggingApp: number | null = null;

  function onAppVolume(a: AppVolume, e: Event) {
    const v = +(e.currentTarget as HTMLInputElement).value / 100;
    a.volume = v;
    a.muted = false;
    api.setAppVolume(a.pid, v, false);
  }

  function toggleAppMute(a: AppVolume) {
    a.muted = !a.muted;
    api.setAppVolume(a.pid, undefined, a.muted);
  }

  // ---- Wi-Fi networks ----
  let wifi = $state<WifiList | null>(null);
  let showWifi = $state(false);
  let wifiBusy = $state<string | null>(null);
  let askPw = $state<string | null>(null); // ssid waiting for a password
  let pw = $state("");
  let wifiErr = $state("");

  async function loadWifi() {
    wifiErr = "";
    try {
      wifi = await api.wifiNetworks();
    } catch (e) {
      wifiErr = String(e);
    }
  }

  async function joinWifi(n: WifiNetwork, password?: string) {
    if (wifiBusy) return;
    wifiBusy = n.ssid;
    wifiErr = "";
    try {
      await api.wifiConnect(n.ssid, password);
      askPw = null;
      pw = "";
      await loadWifi();
      radios = await api.radioStatus();
    } catch (e) {
      if (String(e) === "password") {
        askPw = n.ssid; // secured network Windows doesn't know yet
        if (password) wifiErr = "Password non corretta";
      } else wifiErr = String(e);
    } finally {
      wifiBusy = null;
    }
  }

  async function leaveWifi() {
    wifiBusy = "-";
    try {
      await api.wifiDisconnect();
      await loadWifi();
    } catch (e) {
      wifiErr = String(e);
    } finally {
      wifiBusy = null;
    }
  }

  /** 0..4 bars as a small signal icon */
  function signal(b: number): string {
    const bar = (i: number, h: number) =>
      `<rect x="${3 + i * 5}" y="${20 - h}" width="3" height="${h}" rx="1" opacity="${i < b ? 1 : 0.25}"/>`;
    return `<svg viewBox="0 0 24 24" fill="currentColor">${[5, 9, 13, 17].map((h, i) => bar(i, h)).join("")}</svg>`;
  }

  // ---- output device ----
  let outputs = $state<AudioOut[]>([]);
  let showOut = $state(false);
  let switching = $state<string | null>(null);
  const currentOut = $derived(outputs.find((o) => o.default));

  async function loadOutputs() {
    outputs = await api.audioOutputs();
  }
  onMount(() => {
    loadOutputs();
  });

  async function chooseOutput(o: AudioOut) {
    if (o.default || switching) return;
    switching = o.id;
    error = "";
    try {
      await api.setAudioOutput(o.id);
      await loadOutputs();
      showOut = false;
    } catch (e) {
      error = `Impossibile cambiare l'uscita audio: ${e}`;
    } finally {
      switching = null;
    }
  }

  /** "Cuffie (WH-1000XM5)" -> main "Cuffie", detail "WH-1000XM5" */
  function split(name: string): [string, string] {
    const m = name.match(/^(.*?)\s*\((.*)\)\s*$/);
    return m ? [m[1], m[2]] : [name, ""];
  }

  function onVolume(e: Event) {
    const v = +(e.currentTarget as HTMLInputElement).value / 100;
    if (status) {
      status.volume = v;
      status.muted = false;
    }
    api.setVolume(v);
  }

  function toggleMute() {
    if (!status) return;
    status.muted = !status.muted;
    api.setVolume(undefined, status.muted);
  }

  async function toggleRadio(kind: "wifi" | "bluetooth") {
    if (!radios || radios[kind] === null || busy) return;
    const on = !radios[kind];
    radios[kind] = on; // optimistic
    busy = kind;
    error = "";
    try {
      await api.setRadio(kind, on);
    } catch (e) {
      radios[kind] = !on;
      error = `Impossibile cambiare ${kind === "wifi" ? "il Wi-Fi" : "il Bluetooth"}: ${e}`;
    } finally {
      busy = null;
    }
  }

  const big = $derived([
    {
      kind: "wifi" as const,
      label: "Wi-Fi",
      icon: ico.wifi,
      on: radios?.wifi,
      sub: radios?.wifi ? (radios.network ?? "Non connesso") : "Disattivato",
      title: radios?.wifi && radios.network ? `Connesso a: ${radios.network}` : undefined,
      settings: "ms-settings:network-wifi",
    },
    {
      kind: "bluetooth" as const,
      label: "Bluetooth",
      icon: ico.bluetooth,
      on: radios?.bluetooth,
      // which device we're connected to, if any
      sub: !radios?.bluetooth
        ? "Disattivato"
        : radios.btDevices?.length
          ? radios.btDevices.join(", ")
          : "Nessun dispositivo",
      title: radios?.btDevices?.length ? `Connesso a: ${radios.btDevices.join(", ")}` : undefined,
      settings: "ms-settings:bluetooth",
    },
  ]);
</script>

<div class="audio">
  {#if media}
    <div class="media-card">
      <div class="art" class:playing={media.playing} class:cover={!!art} style={art ? `background-image:url("${art}")` : ""}>
        <span class="eq"><i></i><i></i><i></i><i></i></span>
      </div>
      <div class="meta">
        <b>{media.title}</b>
        <small>{media.artist || media.source}</small>
      </div>
      <div class="ctrl">
        <button class="icon-btn mb" onclick={() => api.mediaAction("prev")}>{@html ico.prev}</button>
        <button class="icon-btn mb main" onclick={() => { if (media) media.playing = !media.playing; api.mediaAction("toggle"); }}>
          {@html media.playing ? ico.pause : ico.play}
        </button>
        <button class="icon-btn mb" onclick={() => api.mediaAction("next")}>{@html ico.next}</button>
      </div>
    </div>
  {/if}

  <div class="toggles">
    {#each big as t (t.kind)}
      <div class="toggle" class:on={t.on} class:missing={t.on === null || t.on === undefined} class:busy={busy === t.kind}>
        <button class="tmain" onclick={() => toggleRadio(t.kind)} disabled={t.on === null} title={t.on ? `Disattiva ${t.label}` : `Attiva ${t.label}`}>
          <span class="ti">{@html t.icon}</span>
          <span class="tt"><b>{t.label}</b><small title={t.title}>{t.on === null ? "Non disponibile" : t.sub}</small></span>
        </button>
        {#if t.kind === "wifi"}
          <button
            class="tmore"
            class:open={showWifi}
            onclick={() => {
              showWifi = !showWifi;
              showOut = false;
              if (showWifi) loadWifi();
            }}
            title="Reti Wi-Fi"
          >{@html ico.chevR}</button>
        {:else}
          <button class="tmore" onclick={() => api.launch(t.settings)} title="Impostazioni {t.label}">{@html ico.chevR}</button>
        {/if}
      </div>
    {/each}
  </div>
  {#if showWifi}
    <div class="outs wifi">
      {#if !wifi && !wifiErr}
        <p class="none">Ricerca delle reti…</p>
      {/if}
      {#if wifi?.savedOnly}
        <p class="wnote">
          La posizione di Windows è disattivata: si vedono solo le reti salvate.
          <button class="link" onclick={() => api.launch("ms-settings:privacy-location")}>Attiva la posizione</button>
          per vedere tutte quelle vicine.
        </p>
      {/if}
      {#each wifi?.networks ?? [] as n (n.ssid)}
        <div class="wrow" class:on={n.connected}>
          <button class="out" class:busy={wifiBusy === n.ssid} onclick={() => (n.connected ? null : joinWifi(n))} title={n.ssid}>
            <span class="oi">{@html wifi?.savedOnly ? ico.wifi : signal(n.bars)}</span>
            <span class="ot">
              <b>{n.ssid}</b>
              <small>{n.connected ? "Connesso" : wifiBusy === n.ssid ? "Connessione…" : wifi?.savedOnly ? "Salvata" : n.secure ? "Protetta" : "Aperta"}</small>
            </span>
            {#if n.connected}<span class="check">{@html ico.check}</span>{/if}
          </button>
          {#if n.connected}
            <button class="wbtn" onclick={leaveWifi} disabled={!!wifiBusy}>Disconnetti</button>
          {/if}
        </div>
        {#if askPw === n.ssid}
          <form
            class="pw"
            onsubmit={(e) => {
              e.preventDefault();
              joinWifi(n, pw);
            }}
          >
            <!-- svelte-ignore a11y_autofocus -->
            <input type="password" bind:value={pw} placeholder="Password di {n.ssid}" autofocus />
            <button class="wbtn primary" disabled={!pw || !!wifiBusy}>Connetti</button>
          </form>
        {/if}
      {:else}
        {#if wifi}<p class="none">Nessuna rete trovata</p>{/if}
      {/each}
      {#if wifiErr}<p class="err">{wifiErr}</p>{/if}
      <button class="more" onclick={() => api.launch("ms-settings:network-wifi")}>Altre impostazioni Wi-Fi</button>
    </div>
  {/if}

  <div class="tiles">
    <button class="tile" class:on={status?.muted} onclick={toggleMute}>
      <span class="ti">{@html status?.muted ? ico.vol0 : ico.vol2}</span><span>{status?.muted ? "Muto" : "Audio"}</span>
    </button>
    <button
      class="tile"
      class:open={showOut}
      onclick={() => {
        showOut = !showOut;
        showWifi = false;
        if (showOut) loadOutputs();
      }}
      title={currentOut ? `Uscita: ${currentOut.name}` : "Uscita audio"}
    >
      <span class="ti">{@html ico.speaker}</span>
      <span class="tl">{currentOut ? split(currentOut.name)[0] : "Uscita audio"}</span>
    </button>
    <button class="tile" onclick={() => api.shell("quick")}>
      <span class="ti">{@html ico.sliders}</span><span>Altro</span>
    </button>
  </div>
  {#if showOut}
    <div class="outs">
      {#each outputs as o (o.id)}
        {@const [main, detail] = split(o.name)}
        <button class="out" class:on={o.default} class:busy={switching === o.id} onclick={() => chooseOutput(o)} title={o.name}>
          <span class="oi">{@html ico.speaker}</span>
          <span class="ot"><b>{main}</b>{#if detail}<small>{detail}</small>{/if}</span>
          {#if o.default}<span class="check">{@html ico.check}</span>{/if}
        </button>
      {:else}
        <p class="none">Nessun dispositivo di uscita attivo</p>
      {/each}
      <button class="more" onclick={() => api.launch("ms-settings:sound")}>Altre impostazioni audio</button>
    </div>
  {/if}
  {#if error}<p class="err">{error}</p>{/if}

  {#if status}
    <div class="slider-row">
      <button class="icon-btn vb" onclick={toggleMute} title={status.muted ? "Riattiva audio" : "Disattiva audio"}>
        {@html volumeIcon(status.volume, status.muted)}
      </button>
      <input
        type="range"
        min="0"
        max="100"
        value={Math.round(status.volume * 100)}
        style="--p:{status.muted ? 0 : status.volume * 100}%"
        oninput={onVolume}
        onpointerdown={() => (dragging = true)}
        onpointerup={() => (dragging = false)}
      />
      <span class="val">{status.muted ? "—" : Math.round(status.volume * 100)}</span>
    </div>

    {#if apps.length}
      <div class="mixer">
        <h6>Volume app</h6>
        {#each apps.slice(0, 6) as a (a.pid)}
          <div class="mrow" class:idle={!a.active}>
            <button class="mi" onclick={() => toggleAppMute(a)} title={a.muted ? `Riattiva ${a.name}` : `Silenzia ${a.name}`}>
              {#if a.exe}
                <AppIcon path={a.exe} name={a.name} size={18} />
              {:else}
                <span class="sysi">{@html ico.bell}</span>
              {/if}
              {#if a.muted}<span class="mute-x">{@html ico.vol0}</span>{/if}
            </button>
            <span class="mn" title={a.name}>{a.name}</span>
            <input
              type="range"
              class="mslider"
              min="0"
              max="100"
              value={Math.round(a.volume * 100)}
              style="--p:{a.muted ? 0 : a.volume * 100}%"
              oninput={(e) => onAppVolume(a, e)}
              onpointerdown={() => (draggingApp = a.pid)}
              onpointerup={() => (draggingApp = null)}
            />
            <span class="mv">{a.muted ? "—" : Math.round(a.volume * 100)}</span>
          </div>
        {/each}
      </div>
    {/if}

    <footer>
      {#if status.battery}
        <span class="bat">
          <span class="bi">{@html batteryIcon(status.battery.percent, status.battery.charging)}</span>
          {status.battery.percent}% {status.battery.charging ? "· in carica" : ""}
        </span>
      {:else}
        <span class="bat">{status.online ? "Connesso a Internet" : "Nessuna connessione"}</span>
      {/if}
      <button class="icon-btn vb" title="Impostazioni" onclick={() => api.shell("settings")}>{@html ico.gear}</button>
    </footer>
  {/if}
</div>

<style>
  .audio {
    /* natural height: the flyout window is fitted to it */
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 18px;
  }
  .media-card {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: 12px;
    background: linear-gradient(135deg, color-mix(in srgb, var(--accent) 22%, transparent), var(--card));
    border: 1px solid var(--stroke);
  }
  .art {
    width: 48px;
    height: 48px;
    border-radius: 10px;
    flex: none;
    display: grid;
    place-items: center;
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 30%, #ff3d9a));
  }
  /* the song's cover; the equalizer stays as a small badge while playing */
  .art.cover {
    background-size: cover;
    background-position: center;
    box-shadow: 0 4px 14px rgba(0, 0, 0, 0.45);
    place-items: end start;
    padding: 4px;
  }
  .art.cover .eq {
    height: 12px;
    gap: 2px;
    padding: 2px 3px;
    border-radius: 4px;
    background: rgba(0, 0, 0, 0.45);
    opacity: 0;
    transition: opacity 0.3s;
  }
  .art.cover.playing .eq {
    opacity: 1;
  }
  .art.cover .eq i {
    width: 2px;
  }
  .eq {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 20px;
  }
  .eq i {
    width: 3px;
    height: 30%;
    background: white;
    border-radius: 2px;
  }
  .art.playing .eq i {
    animation: eq 0.8s ease-in-out infinite alternate;
  }
  .art.playing .eq i:nth-child(2) { animation-delay: -0.2s; }
  .art.playing .eq i:nth-child(3) { animation-delay: -0.5s; }
  .art.playing .eq i:nth-child(4) { animation-delay: -0.7s; }
  @keyframes eq {
    from { height: 15%; }
    to { height: 100%; }
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .meta b,
  .meta small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta small {
    color: var(--fg-dim);
  }
  .ctrl {
    display: flex;
    align-items: center;
  }
  .mb {
    width: 30px;
    height: 30px;
    padding: 7px;
  }
  .mb.main {
    width: 36px;
    height: 36px;
    padding: 9px;
    border-radius: 50%;
    background: var(--fg);
    color: var(--bar-bg-solid);
  }
  .toggles {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .toggle {
    display: flex;
    height: 58px;
    border-radius: 14px;
    background: var(--card);
    box-shadow: inset 0 0 0 1px var(--stroke);
    overflow: hidden;
    transition: background 0.3s, box-shadow 0.3s;
  }
  .toggle.on {
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 55%, var(--aurora-2)));
    box-shadow: 0 6px 18px -8px color-mix(in srgb, var(--accent) 80%, transparent);
    color: #0b0b12;
  }
  .toggle.missing {
    opacity: 0.45;
  }
  .toggle.busy .ti {
    animation: pulse 0.9s ease-in-out infinite alternate;
  }
  @keyframes pulse {
    to {
      opacity: 0.35;
    }
  }
  .tmain {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 6px 0 12px;
    text-align: left;
  }
  .tmain:hover,
  .tmore:hover {
    background: rgba(255, 255, 255, 0.08);
  }
  .toggle.on .tmain:hover,
  .toggle.on .tmore:hover {
    background: rgba(0, 0, 0, 0.08);
  }
  .tt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }
  .tt b {
    font-weight: 600;
    font-size: 12.5px;
  }
  .tt small {
    font-size: 11px;
    line-height: 1.2;
    opacity: 0.75;
    /* device / network names: up to two lines before the ellipsis */
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
  }
  .toggle.on .ti {
    color: #0b0b12;
  }
  .tmore {
    width: 30px;
    padding: 0 8px;
    border-left: 1px solid rgba(255, 255, 255, 0.08);
  }
  .toggle.on .tmore {
    border-left-color: rgba(0, 0, 0, 0.12);
  }
  .err {
    margin: -4px 2px 0;
    font-size: 11.5px;
    color: #ff8a8a;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }
  .tl {
    max-width: 100%;
    padding: 0 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tile.open {
    background: color-mix(in srgb, var(--accent) 16%, var(--card));
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }
  /* ---- output device list ---- */
  .outs {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: -6px;
    padding: 6px;
    border-radius: 12px;
    background: var(--card);
    border: 1px solid var(--stroke);
    animation: drop 0.22s var(--ease);
  }
  @keyframes drop {
    from {
      opacity: 0;
      transform: translateY(-6px);
    }
  }
  .out {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: 8px;
    text-align: left;
  }
  .out:hover {
    background: var(--hover);
  }
  .out.on {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .out.busy .oi {
    animation: pulse 0.9s ease-in-out infinite alternate;
  }
  .oi {
    width: 18px;
    height: 18px;
    flex: none;
    color: var(--fg-dim);
  }
  .out.on .oi,
  .check {
    color: var(--accent);
  }
  .ot {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .ot b,
  .ot small {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ot b {
    font-weight: 600;
    font-size: 12.5px;
  }
  .ot small {
    font-size: 11px;
    color: var(--fg-dim);
  }
  .check {
    width: 16px;
    height: 16px;
    flex: none;
  }
  .none {
    margin: 6px 8px;
    color: var(--fg-dim);
    font-size: 12px;
  }
  .more {
    margin-top: 2px;
    padding: 6px 8px;
    border-radius: 8px;
    text-align: left;
    font-size: 12px;
    color: var(--accent);
  }
  .more:hover {
    background: var(--hover);
  }
  /* ---- Wi-Fi list ---- */
  .outs.wifi {
    max-height: 260px;
    overflow-y: auto;
  }
  .tmore.open {
    background: rgba(0, 0, 0, 0.12);
  }
  .wrow {
    display: flex;
    align-items: center;
    gap: 6px;
    border-radius: 8px;
  }
  .wrow .out {
    flex: 1;
    min-width: 0;
  }
  .wrow.on {
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .wrow.on .out {
    background: none;
  }
  .wbtn {
    flex: none;
    height: 26px;
    padding: 0 10px;
    margin-right: 6px;
    border-radius: 7px;
    font-size: 11.5px;
    background: var(--card);
    box-shadow: inset 0 0 0 1px var(--stroke);
  }
  .wbtn:hover {
    background: var(--hover);
  }
  .wbtn.primary {
    background: var(--accent);
    color: #0b0b12;
    font-weight: 600;
    box-shadow: none;
  }
  .wbtn:disabled {
    opacity: 0.5;
  }
  .pw {
    display: flex;
    gap: 6px;
    padding: 2px 6px 6px 36px;
  }
  .pw input {
    flex: 1;
    min-width: 0;
    height: 28px;
    padding: 0 10px;
    border-radius: 7px;
    border: 1px solid var(--stroke);
    background: rgba(0, 0, 0, 0.25);
    color: var(--fg);
    font: inherit;
    outline: none;
  }
  .pw input:focus {
    border-color: var(--accent);
  }
  .wnote {
    margin: 2px 8px 6px;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--fg-dim);
  }
  .link {
    color: var(--accent);
    text-decoration: underline;
    font-size: inherit;
  }
  /* ---- per-app volume ---- */
  .mixer {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-top: -4px;
  }
  h6 {
    margin: 0 2px 4px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }
  .mrow {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 30px;
  }
  .mrow.idle .mn {
    color: var(--fg-dim);
  }
  .mi {
    position: relative;
    width: 26px;
    height: 26px;
    flex: none;
    display: grid;
    place-items: center;
    border-radius: 7px;
  }
  .mi:hover {
    background: var(--hover);
  }
  .sysi {
    width: 16px;
    height: 16px;
    color: var(--fg-dim);
  }
  .mute-x {
    position: absolute;
    right: -2px;
    bottom: -2px;
    width: 13px;
    height: 13px;
    padding: 1px;
    border-radius: 50%;
    background: #ff5f6d;
    color: #fff;
  }
  .mn {
    width: 92px;
    flex: none;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mslider {
    height: 3px !important;
  }
  .mslider::-webkit-slider-thumb {
    width: 14px !important;
    height: 14px !important;
    border-width: 3px !important;
  }
  .mv {
    width: 24px;
    flex: none;
    text-align: right;
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--fg-dim);
  }
  .tile.on {
    background: color-mix(in srgb, #ff5f6d 25%, transparent);
  }
  .tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 12px 4px;
    border-radius: 10px;
    background: var(--card);
    border: 1px solid var(--stroke);
    font-size: 11.5px;
    transition: background 0.15s, transform 0.2s var(--ease);
  }
  .tile:hover {
    background: var(--hover);
    transform: translateY(-2px);
  }
  .ti {
    width: 20px;
    height: 20px;
    color: var(--accent);
  }
  .slider-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .vb {
    width: 32px;
    height: 32px;
    padding: 7px;
    flex: none;
  }
  .val {
    width: 28px;
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--fg-dim);
  }
  input[type="range"] {
    flex: 1;
    appearance: none;
    height: 4px;
    border-radius: 4px;
    background: linear-gradient(90deg, var(--accent) var(--p), var(--stroke) var(--p));
    outline: none;
  }
  input[type="range"]::-webkit-slider-thumb {
    appearance: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--accent);
    border: 4px solid var(--bar-bg-solid);
    box-shadow: 0 0 0 1px var(--stroke);
    transition: transform 0.15s var(--ease);
  }
  input[type="range"]::-webkit-slider-thumb:hover {
    transform: scale(1.2);
  }
  footer {
    margin-top: auto;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 10px;
    border-top: 1px solid var(--stroke);
  }
  .bat {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--fg-dim);
  }
  .bi {
    width: 20px;
    height: 20px;
  }
</style>
