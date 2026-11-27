<script lang="ts">
  import { onMount } from "svelte";
  import { api, type JumpItem, type Settings } from "$lib/api";
  import type { Group } from "$lib/groups";
  import { ico } from "$lib/icons";
  import AppIcon from "./AppIcon.svelte";

  let { group, settings }: { group: Group; settings: Settings } = $props();

  // local copy: closing a window removes it immediately from the list
  // svelte-ignore state_referenced_locally
  let windows = $state(group.windows.slice());
  const pinned = $derived(settings.pinned.some((p) => p.appId.toLowerCase() === group.appId));

  function activate(hwnd: number) {
    api.windowAction(hwnd, "activate");
    api.closeFlyout();
  }

  function close(hwnd: number, e: Event) {
    e.stopPropagation();
    api.windowAction(hwnd, "close");
    windows = windows.filter((w) => w.hwnd !== hwnd);
    if (windows.length === 0) api.closeFlyout();
  }

  function closeAll() {
    windows.forEach((w) => api.windowAction(w.hwnd, "close"));
    api.closeFlyout();
  }

  // ---- recent items & tasks (the app's jump list) ----
  let jump = $state<JumpItem[]>([]);
  const recent = $derived(jump.filter((j) => !j.task).slice(0, 6));
  const tasks = $derived(jump.filter((j) => j.task).slice(0, 3));
  onMount(() => {
    api
      .jumpList(group.windows[0]?.hwnd, group.target, group.windows[0]?.exe ?? "")
      .then((j) => (jump = j))
      .catch(() => {});
  });

  function openJump(j: JumpItem) {
    if (j.link) api.launch(j.path, j.args || undefined);
    // a recent file/folder: open it with this app (Store apps: its default handler)
    else if (group.target.toLowerCase().startsWith("shell:")) api.launch(j.path);
    else api.launch(group.target, `"${j.path}"`);
    api.closeFlyout();
  }

  /** folder-looking entries get a folder icon */
  const isFolder = (j: JumpItem) =>
    j.args.includes("--folder-uri") || (!j.link && !/\.[a-z0-9]{1,5}$/i.test(j.path));

  const pinIndex = $derived(settings.pinned.findIndex((p) => p.appId.toLowerCase() === group.appId));

  /** Moves this pinned app one place left (-1) or right (+1) on the bar. */
  async function move(dir: -1 | 1) {
    const s = $state.snapshot(settings);
    const i = s.pinned.findIndex((p) => p.appId.toLowerCase() === group.appId);
    const j = i + dir;
    if (i < 0 || j < 0 || j >= s.pinned.length) return;
    [s.pinned[i], s.pinned[j]] = [s.pinned[j], s.pinned[i]];
    await api.saveSettings(s); // the panel stays open: click again to keep moving
  }

  async function togglePin() {
    const s = $state.snapshot(settings);
    if (pinned) {
      s.pinned = s.pinned.filter((p) => p.appId.toLowerCase() !== group.appId);
    } else {
      s.pinned.push({ name: group.name, target: group.target, appId: group.appId });
    }
    await api.saveSettings(s);
    api.closeFlyout();
  }
</script>

<div class="group">
  <header>
    <AppIcon path={group.iconPath} hwnd={group.windows[0]?.hwnd} name={group.name} size={28} />
    <b>{group.name}</b>
  </header>

  <!-- windows + recent items scroll; the actions below always stay visible -->
  <div class="scroll">
  {#if windows.length}
    <ul>
      {#each windows as w (w.hwnd)}
        <li>
          <button class="win" onclick={() => activate(w.hwnd)} title={w.title}>
            <span class="t">{w.title}</span>
            {#if w.attention}<small class="att">● nuovo</small>{:else if w.minimized}<small>ridotta</small>{/if}
            <span class="x icon-btn" role="button" tabindex="-1" onclick={(e) => close(w.hwnd, e)} onkeydown={() => {}} title="Chiudi finestra">
              {@html ico.close}
            </span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  {#if recent.length}
    <h5>Recenti</h5>
    <ul class="jump">
      {#each recent as j (j.path + j.args)}
        <li>
          <button class="jitem" onclick={() => openJump(j)} title={j.link ? j.args.replace(/^.*?"(.*)".*$/, "$1") : j.path}>
            <span class="ji">{@html isFolder(j) ? ico.folder : ico.file}</span>
            <span class="t">{j.name}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  {#if tasks.length}
    <h5>Attività</h5>
    <ul class="jump">
      {#each tasks as j (j.path + j.args)}
        <li>
          <button class="jitem" onclick={() => openJump(j)}>
            <span class="ji">{@html ico.bolt}</span>
            <span class="t">{j.name}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
  </div>

  <div class="actions">
    <button class="act" onclick={() => api.launch(group.target)}>
      <span class="ai">{@html ico.plus}</span>Nuova finestra
    </button>
    {#if pinned}
      <div class="move">
        <button class="act" onclick={() => move(-1)} disabled={pinIndex <= 0} title="Sposta a sinistra sulla barra">
          <span class="ai">{@html ico.chevL}</span>Sposta a sinistra
        </button>
        <button class="act" onclick={() => move(1)} disabled={pinIndex >= settings.pinned.length - 1} title="Sposta a destra sulla barra">
          Sposta a destra<span class="ai">{@html ico.chevR}</span>
        </button>
      </div>
    {/if}
    <button class="act" onclick={togglePin}>
      <span class="ai">{@html pinned ? ico.unpin : ico.pin}</span>{pinned ? "Rimuovi dalla barra" : "Aggiungi alla barra"}
    </button>
    {#if windows.length}
      <button class="act danger" onclick={closeAll}>
        <span class="ai">{@html ico.close}</span>{windows.length > 1 ? "Chiudi tutte le finestre" : "Chiudi finestra"}
      </button>
    {/if}
  </div>
</div>

<style>
  .group {
    /* natural height (the window is fitted to it), long lists scroll */
    max-height: 600px;
    display: flex;
    flex-direction: column;
    padding: 12px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px 12px;
  }
  header b {
    font-weight: 600;
    font-size: 14px;
  }
  ul {
    list-style: none;
    margin: 0 0 8px;
    padding: 0;
    min-height: 0;
    overflow-y: auto;
  }
  .win {
    width: 100%;
    height: 42px;
    margin-bottom: 4px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 6px 0 12px;
    border-radius: 8px;
    background: var(--card);
    border: 1px solid var(--stroke);
    text-align: left;
    transition: background 0.15s;
  }
  .win:hover {
    background: var(--hover);
  }
  .t {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  small {
    color: var(--fg-faint);
    font-size: 11px;
  }
  .x {
    width: 28px;
    height: 28px;
    padding: 8px;
    opacity: 0;
  }
  .win:hover .x {
    opacity: 1;
  }
  .x:hover {
    background: #c42b1c;
    color: white;
  }
  .actions {
    margin-top: auto;
    border-top: 1px solid var(--stroke);
    padding-top: 6px;
  }
  .act {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 10px;
    border-radius: 8px;
  }
  .act:hover {
    background: var(--hover);
  }
  .act.danger:hover {
    color: #ff6b6b;
  }
  .scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
  }
  .actions {
    flex: none;
  }
  h5 {
    margin: 4px 10px 4px;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-faint);
  }
  ul.jump {
    margin-bottom: 6px;
    flex: none;
  }
  .jitem {
    width: 100%;
    height: 32px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 10px;
    border-radius: 8px;
    text-align: left;
  }
  .jitem:hover {
    background: var(--hover);
  }
  .ji {
    width: 16px;
    height: 16px;
    flex: none;
    color: var(--accent);
  }
  .att {
    color: #ffa53d;
    font-weight: 600;
  }
  .move {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
  }
  .move .act {
    gap: 6px;
    padding: 0 8px;
    font-size: 12.5px;
  }
  .move .act:last-child {
    justify-content: flex-end;
  }
  .act:disabled {
    opacity: 0.35;
    pointer-events: none;
  }
  .ai {
    width: 16px;
    height: 16px;
  }
</style>
