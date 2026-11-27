<script lang="ts">
  import "$lib/global.css";
  import { onMount } from "svelte";
  import { followTheme } from "$lib/theme";
  import { api, isTauri } from "$lib/api";
  let { children } = $props();

  // every window (bar, panels, previews) wears the same colour theme
  onMount(followTheme);

  // Alt+F4 on the bar does what it does on Windows' taskbar: the "Shut down
  // Windows" dialog (it must never close the bar by accident)
  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey && e.key === "F4") {
        e.preventDefault();
        if (isTauri) api.shell("shutdown");
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  });
</script>

{@render children()}
