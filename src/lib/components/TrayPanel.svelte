<script lang="ts">
  import { onMount } from "svelte";
  import { api, listen, type TrayItem } from "$lib/api";

  let items = $state<TrayItem[]>([]);
  let hover = $state<TrayItem | null>(null);
  let error = $state("");

  onMount(() => {
    api.trayIcons().then((t) => (items = t));
    const un = listen<TrayItem[]>("tray-changed", (t) => (items = t));
    return () => {
      un.then((f) => f());
    };
  });

  /** Screen position in physical pixels: apps use it to place their menus. */
  function at(e: MouseEvent): [number, number] {
    const k = window.devicePixelRatio || 1;
    return [Math.round(e.screenX * k), Math.round(e.screenY * k)];
  }

  // apps whose icon Windows kept (they registered before the bar started):
  // their own menu is out of reach, so right-click shows a small one of ours
  let menuFor = $state<TrayItem | null>(null);
  const isOrphan = (it: TrayItem) => it.id.startsWith("orphan:");

  async function send(e: MouseEvent, it: TrayItem, kind: "left" | "right" | "double") {
    e.preventDefault();
    error = "";
    if (kind === "right" && isOrphan(it)) {
      menuFor = menuFor?.id === it.id ? null : it;
      return;
    }
    menuFor = null;
    try {
      await api.trayClick(it.id, kind, ...at(e));
    } catch (err) {
      error = String(err);
    }
  }

  const label = (it: TrayItem) => (it.tip || "App").split("\n")[0];
</script>

<div class="tray">
  {#if items.length}
    <div class="grid">
      {#each items as it, i (it.id)}
        <button
          class="ic"
          style="animation-delay:{i * 18}ms"
          aria-label={label(it)}
          onmouseenter={() => (hover = it)}
          onmouseleave={() => (hover = null)}
          onclick={(e) => send(e, it, "left")}
          ondblclick={(e) => send(e, it, "double")}
          oncontextmenu={(e) => send(e, it, "right")}
        >
          {#if it.icon}
            <img src={it.icon} alt="" draggable="false" />
          {:else}
            <span class="letter">{label(it).trim().charAt(0).toUpperCase()}</span>
          {/if}
        </button>
      {/each}
    </div>
    {#if menuFor}
      <div class="mini-menu">
        <button
          class="mm"
          onclick={(e) => {
            const it = menuFor!;
            menuFor = null;
            send(e, it, "left");
          }}
        >Apri {label(menuFor)}</button>
        <p class="mm-note">
          Il menu completo di quest'app comparirà quando verrà riaperta con la barra attiva (es. chiudendola e riaprendola da Start).
        </p>
      </div>
    {:else}
      <p class="name" class:err={!!error}>
        {error || (hover ? label(hover) : "Clic destro per il menu")}
      </p>
    {/if}
  {:else}
    <p class="name">Nessuna app in background</p>
  {/if}
</div>

<style>
  .tray {
    /* natural height: the window is fitted to it (error lines can wrap) */
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 6px;
    padding: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(5, 34px);
    gap: 4px;
    justify-content: center;
  }
  .ic {
    position: relative;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    border-radius: 10px;
    transition: background 0.15s, transform 0.2s var(--ease);
    animation: pop 0.28s var(--ease) both;
  }
  .ic::after {
    content: "";
    position: absolute;
    inset: 0;
    border-radius: inherit;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.08);
    opacity: 0;
    transition: opacity 0.15s;
  }
  .ic:hover {
    background:
      radial-gradient(70% 70% at 50% 100%, color-mix(in srgb, var(--accent) 30%, transparent), transparent 75%),
      rgba(255, 255, 255, 0.07);
    transform: translateY(-1px);
  }
  .ic:hover::after {
    opacity: 1;
  }
  .ic:active {
    transform: scale(0.88);
  }
  .ic img,
  .letter {
    width: 18px;
    height: 18px;
  }
  .ic img {
    object-fit: contain;
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.4));
  }
  .letter {
    display: grid;
    place-items: center;
    border-radius: 5px;
    font-size: 10px;
    font-weight: 700;
    color: #fff;
    background: linear-gradient(135deg, var(--accent), var(--aurora-2));
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(6px) scale(0.8);
    }
  }
  .name {
    margin: 0;
    height: 16px;
    text-align: center;
    font-size: 11px;
    color: var(--fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mini-menu {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 4px;
    border-radius: 10px;
    background: var(--card);
    border: 1px solid var(--stroke);
    animation: pop 0.2s var(--ease) both;
  }
  .mm {
    padding: 7px 8px;
    border-radius: 7px;
    text-align: left;
    font-size: 12px;
    font-weight: 600;
  }
  .mm:hover {
    background: var(--hover);
  }
  .mm-note {
    margin: 0 6px 4px;
    font-size: 10.5px;
    line-height: 1.35;
    color: var(--fg-dim);
  }
  .name.err {
    height: auto;
    white-space: normal;
    line-height: 1.3;
    color: #ff8a8a;
  }
</style>
