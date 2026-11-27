<script lang="ts">
  import { iconFor } from "$lib/api";

  let { path = "", hwnd = undefined, name = "", size = 26 }: {
    path?: string;
    hwnd?: number;
    name?: string;
    size?: number;
  } = $props();

  let src = $state<string | null>(null);
  let failed = $state(false);
  let shownFor = "";

  $effect(() => {
    const p = path;
    const h = hwnd;
    const sz = size;
    const key = `${p}|${h ?? ""}`;
    // a different app: clear; only a different size: keep the current image
    // until the sharper one arrives (no flicker when the icon grows/shrinks)
    if (key !== shownFor) {
      src = null;
      failed = false;
    }
    iconFor(p, h, sz).then((url) => {
      if (p !== path || sz !== size) return;
      if (url) {
        src = url;
        failed = false;
      } else if (!src) {
        failed = true;
      }
      shownFor = key;
    });
  });

  const letter = $derived((name || "?").trim().charAt(0).toUpperCase());
</script>

<span class="app-icon" style="--s:{size}px">
  {#if src}
    <img {src} alt="" draggable="false" />
  {:else if failed}
    <span class="fallback">{letter}</span>
  {/if}
</span>

<style>
  .app-icon {
    width: var(--s);
    height: var(--s);
    display: grid;
    place-items: center;
    flex: none;
    transition:
      width 0.28s cubic-bezier(0.3, 0.9, 0.3, 1.2),
      height 0.28s cubic-bezier(0.3, 0.9, 0.3, 1.2);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: contain;
    filter: drop-shadow(0 1px 2px rgba(0, 0, 0, 0.25));
  }
  .fallback {
    width: 100%;
    height: 100%;
    border-radius: 28%;
    display: grid;
    place-items: center;
    font-weight: 600;
    font-size: calc(var(--s) * 0.5);
    color: white;
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 50%, #5c2d91));
  }
</style>
