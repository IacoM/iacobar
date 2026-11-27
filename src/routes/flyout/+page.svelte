<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, isTauri, listen, type Settings } from "$lib/api";
  import StartPanel from "$lib/components/StartPanel.svelte";
  import CalendarPanel from "$lib/components/CalendarPanel.svelte";
  import AudioPanel from "$lib/components/AudioPanel.svelte";
  import GroupPanel from "$lib/components/GroupPanel.svelte";
  import SettingsPanel from "$lib/components/SettingsPanel.svelte";
  import TrayPanel from "$lib/components/TrayPanel.svelte";  import type { Group } from "$lib/groups";

  let panel = $state("");
  let data = $state<any>(null);
  let openId = $state(0);
  let settings = $state<Settings | null>(null);
  let leaving = $state(false); // fading out before the window hides
  let leaveTimer = 0;

  // sizes used only by the browser preview (Tauri sizes the real window)
  const mockSize: Record<string, [number, number]> = {
    start: [640, 620],
    calendar: [340, 480],
    audio: [360, 390],
    group: [320, 330],
    settings: [340, 420],
    tray: [214, 114],  };

  onMount(() => {
    api.getSettings().then((s) => (settings = s));
    const un = [
      listen<{ panel: string; data: unknown }>("flyout-open", (p) => {
        clearTimeout(unmountTimer);
        clearTimeout(leaveTimer);
        leaving = false;
        panel = p.panel;
        settled = !FIT.has(p.panel); // fitted panels show up once sized
        data = p.data;
        openId++;
        api.getSettings().then((s) => (settings = s));
      }),
      listen<Settings>("settings-changed", (s) => (settings = s)),
    ];

    if (!isTauri) {
      const q = new URLSearchParams(location.search);
      panel = q.get("panel") ?? "start";
      settled = !FIT.has(panel);
      if (panel === "group") {
        data = {
          appId: "c:\\apps\\msedge.exe", name: "Msedge", target: "C:\\Apps\\msedge.exe",
          iconPath: "C:\\Apps\\msedge.exe", pinned: true,
          windows: [
            { hwnd: 103, title: "Tauri docs - Edge", exe: "", appId: "", minimized: false },
            { hwnd: 104, title: "GitHub - Edge", exe: "", appId: "", minimized: true },
          ],
        };
      }
    }

    // Closing: the panel fades out FIRST, then the window is hidden. The
    // last frame of a hidden window is what Windows shows for an instant when
    // it opens again: it must be an empty one, not the old panel.
    let unmountTimer = 0;
    un.push(
      listen("flyout-close", () => {
        if (leaving) return;
        leaving = true;
        clearTimeout(leaveTimer);
        leaveTimer = setTimeout(() => {
          api.hideFlyout();
          // unmount once hidden: nothing keeps running in an invisible window
          clearTimeout(unmountTimer);
          unmountTimer = setTimeout(() => (panel = ""), 250) as unknown as number;
        }, 130) as unknown as number;
      }),
    );
    const close = () => api.closeFlyout();
    const onBlur = () => isTauri && close();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && close();
    window.addEventListener("blur", onBlur);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("keydown", onKey);
      un.forEach((p) => p.then((f) => f()));
    };
  });

  // panels whose height depends on their content (music playing or not,
  // error lines...): the window follows the content, no empty band
  const FIT = new Set(["audio", "group", "tray"]);
  let contentEl = $state<HTMLElement>();
  // A fitted panel stays invisible while its content arrives in waves (media,
  // radios, mixer...) and the window is resized to it: then it pops in once,
  // at its final size, instead of jumping around during the intro animation.
  let settled = $state(true);
  $effect(() => {
    // the panel's element appears only once its data has loaded
    void settings;
    void data;
    const el = contentEl?.firstElementChild as HTMLElement | null;
    if (!el || !FIT.has(panel)) return;
    let t = 0;
    let fallback = 0;
    let waiting = !untrack(() => settled);
    const fit = async () => {
      // layout height: unlike getBoundingClientRect it ignores the intro's scale
      const h = el.offsetHeight;
      if (isTauri) await api.fitFlyout(h).catch(() => {});
      else fitMock = h;
      if (waiting) {
        waiting = false;
        clearTimeout(fallback);
        setTimeout(() => (settled = true), 16); // reveal right after the resize
      }
    };
    const ro = new ResizeObserver(() => {
      clearTimeout(t);
      // while hidden: wait for the size to stay still a moment
      t = setTimeout(fit, waiting ? 160 : 40) as unknown as number;
    });
    ro.observe(el);
    // never keep a panel hidden for long (slow data source)
    if (waiting) fallback = setTimeout(fit, 700) as unknown as number;
    return () => {
      clearTimeout(t);
      clearTimeout(fallback);
      ro.disconnect();
    };
  });
  let fitMock = $state(0);

  function switchPanel(p: string) {
    panel = p;
    openId++;
  }
</script>

<div class="stage" class:mock={!isTauri}>
  <div
    class="panel"
    class:waiting={!settled}
    class:leaving
    style={!isTauri && mockSize[panel]
      ? `width:${mockSize[panel][0]}px;height:${FIT.has(panel) && fitMock ? fitMock : mockSize[panel][1]}px`
      : ""}
  >
    {#key openId}
      <div class="content" bind:this={contentEl}>
        {#if panel === "start" && settings}
          <StartPanel {settings} focusSearch={!!data?.focusSearch} onSettings={() => switchPanel("settings")} />
        {:else if panel === "calendar"}
          <CalendarPanel />
        {:else if panel === "audio"}
          <AudioPanel />
        {:else if panel === "group" && data && settings}
          <GroupPanel group={data as Group} {settings} />
        {:else if panel === "tray"}
          <TrayPanel />        {:else if panel === "settings" && settings}
          <SettingsPanel {settings} onBack={() => switchPanel("start")} />
        {/if}
      </div>
    {/key}
  </div>
</div>

<style>
  .stage {
    height: 100vh;
  }
  .stage.mock {
    display: grid;
    place-items: end center;
    padding-bottom: 20px;
    background:
      radial-gradient(1200px 600px at 20% 10%, #3a5bd9 0%, transparent 60%),
      radial-gradient(900px 500px at 85% 30%, #b83cc6 0%, transparent 55%),
      linear-gradient(160deg, #10162f, #1d1036);
  }
  .panel {
    position: relative;
    width: 100%;
    height: 100%;
    border-radius: 22px;
    overflow: hidden;
    background: var(--glass);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      inset 0 0 0 1px rgba(255, 255, 255, 0.07);
  }
  :global(:root[data-scheme="light"]) .panel {
    background:
      radial-gradient(500px 220px at 0% 0%, color-mix(in srgb, var(--accent) 14%, transparent), transparent 70%),
      linear-gradient(180deg, var(--glass-1), var(--glass-2));
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.08);
  }
  .content {
    height: 100%;
  }
  /* the intro plays only once the panel is sized (see `settled`) */
  .panel:not(.waiting) .content {
    animation: pop 0.28s var(--ease);
  }
  .panel.waiting {
    opacity: 0;
  }
  .panel.leaving {
    opacity: 0;
    transform: translateY(6px) scale(0.99);
    transition:
      opacity 0.12s ease-in,
      transform 0.12s ease-in;
  }
  .panel:not(.waiting) {
    animation: panel-in 0.2s ease-out;
  }
  @keyframes panel-in {
    from {
      opacity: 0;
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.98);
    }
  }
</style>
