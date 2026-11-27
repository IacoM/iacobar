<script lang="ts">
  import { onMount, tick } from "svelte";
  import { api, isTauri, listen, type WinInfo } from "$lib/api";
  import { ico } from "$lib/icons";
  import AppIcon from "$lib/components/AppIcon.svelte";

  interface PreviewData {
    name: string;
    iconPath: string;
    windows: WinInfo[];
  }

  let data = $state<PreviewData | null>(null);
  let openId = $state(0);
  let cards: HTMLElement[] = $state([]);

  async function syncThumbs() {
    await tick();
    if (!data) return;
    const rects = data.windows.map((w, i) => {
      const r = cards[i]?.getBoundingClientRect();
      return { hwnd: w.hwnd, x: r?.left ?? 0, y: r?.top ?? 0, w: r?.width ?? 0, h: r?.height ?? 0 };
    });
    api.setThumbs(rects);
  }

  onMount(() => {
    const un = listen<{ data: PreviewData }>("preview-open", async (p) => {
      data = p.data;
      openId++;
      // wait for layout + the window resize before placing thumbnails
      await syncThumbs();
      setTimeout(syncThumbs, 60);
      setTimeout(syncThumbs, 250);
    });
    const onResize = () => syncThumbs();
    window.addEventListener("resize", onResize);

    if (!isTauri) {
      data = {
        name: "Msedge",
        iconPath: "C:\\Apps\\msedge.exe",
        windows: [
          { hwnd: 103, title: "Tauri docs - Edge", exe: "", appId: "", minimized: false, monitor: 1 },
          { hwnd: 104, title: "GitHub - Edge", exe: "", appId: "", minimized: true, monitor: 1 },
        ],
      };
    }
    return () => {
      window.removeEventListener("resize", onResize);
      un.then((f) => f());
    };
  });

  function open(w: WinInfo) {
    api.windowAction(w.hwnd, "activate");
  }

  function close(w: WinInfo, e: Event) {
    e.stopPropagation();
    api.windowAction(w.hwnd, "close");
  }
</script>

<div
  class="stage"
  class:mock={!isTauri}
  role="presentation"
  onmouseenter={() => api.previewHold(true)}
  onmouseleave={() => { api.previewHold(false); api.hidePreview(200); }}
>
  {#if data}
    {#key openId}
      <div class="panel">
        {#each data.windows as w, i (w.hwnd)}
          <button class="card" onclick={() => open(w)} style="animation-delay:{i * 35}ms">
            <div class="head">
              <AppIcon path={data.iconPath} hwnd={w.hwnd} name={data.name} size={16} />
              <span class="t">{w.title}</span>
              <span class="x" role="button" tabindex="-1" title="Chiudi" onclick={(e) => close(w, e)} onkeydown={() => {}}>
                {@html ico.close}
              </span>
            </div>
            <div class="thumb" bind:this={cards[i]}>
              {#if w.minimized}
                <div class="min">
                  <AppIcon path={data.iconPath} hwnd={w.hwnd} name={data.name} size={40} />
                  <small>Ridotta a icona</small>
                </div>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    {/key}
  {/if}
</div>

<style>
  .stage {
    height: 100vh;
  }
  .stage.mock {
    display: grid;
    place-items: center;
    background: linear-gradient(160deg, #10162f, #1d1036);
  }
  .panel {
    height: 100%;
    display: flex;
    gap: 10px;
    padding: 12px;
    border-radius: 20px;
    background: var(--glass);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  .stage.mock .panel {
    height: 204px;
  }
  :global(:root[data-scheme="light"]) .panel {
    background: linear-gradient(180deg, var(--glass-1), var(--glass-2));
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
  }
  .card {
    width: 240px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px;
    border-radius: 12px;
    text-align: left;
    transition: background 0.15s;
    animation: in 0.22s var(--ease) both;
  }
  .card:hover .thumb {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 70%, transparent);
  }
  .card:hover {
    background: var(--hover);
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 22px;
    padding: 0 2px 0 4px;
  }
  .t {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .x {
    width: 22px;
    height: 22px;
    padding: 6px;
    border-radius: 6px;
    opacity: 0;
    transition: opacity 0.15s, background 0.15s;
  }
  .card:hover .x {
    opacity: 1;
  }
  .x:hover {
    background: #c42b1c;
    color: #fff;
  }
  /* the live DWM thumbnail is drawn by Windows on top of this box */
  .thumb {
    flex: 1;
    border-radius: 8px;
    background: rgba(255, 255, 255, 0.04);
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .min {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--fg-dim);
  }
  .min small {
    font-size: 11px;
  }
</style>
