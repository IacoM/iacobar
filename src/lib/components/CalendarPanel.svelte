<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import { ico } from "$lib/icons";

  const start = new Date();
  let now = $state(start);
  let view = $state(new Date(start.getFullYear(), start.getMonth(), 1));

  onMount(() => {
    const t = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(t);
  });

  const days = ["lun", "mar", "mer", "gio", "ven", "sab", "dom"];

  const cells = $derived.by(() => {
    const y = view.getFullYear();
    const m = view.getMonth();
    const offset = (new Date(y, m, 1).getDay() + 6) % 7; // Monday first
    const start = new Date(y, m, 1 - offset);
    return Array.from({ length: 42 }, (_, i) => {
      const d = new Date(start.getFullYear(), start.getMonth(), start.getDate() + i);
      return {
        d,
        out: d.getMonth() !== m,
        today: d.toDateString() === now.toDateString(),
        weekend: d.getDay() === 0 || d.getDay() === 6,
      };
    });
  });

  const move = (n: number) => (view = new Date(view.getFullYear(), view.getMonth() + n, 1));
  const title = $derived(view.toLocaleDateString("it-IT", { month: "long", year: "numeric" }));
  const time = $derived(now.toLocaleTimeString("it-IT"));
  const longDate = $derived(now.toLocaleDateString("it-IT", { weekday: "long", day: "numeric", month: "long" }));

  // ISO week number, a small nicety
  const week = $derived.by(() => {
    const d = new Date(Date.UTC(now.getFullYear(), now.getMonth(), now.getDate()));
    d.setUTCDate(d.getUTCDate() + 4 - (d.getUTCDay() || 7));
    const y0 = new Date(Date.UTC(d.getUTCFullYear(), 0, 1));
    return Math.ceil(((d.getTime() - y0.getTime()) / 86400000 + 1) / 7);
  });
</script>

<div class="cal">
  <header>
    <div class="time">{time}</div>
    <div class="date">{longDate} · settimana {week}</div>
  </header>

  <div class="nav">
    <b>{title}</b>
    <div>
      <button class="icon-btn nb" onclick={() => move(-1)} title="Mese precedente">{@html ico.chevL}</button>
      <button class="today-btn" onclick={() => (view = new Date(now.getFullYear(), now.getMonth(), 1))}>Oggi</button>
      <button class="icon-btn nb" onclick={() => move(1)} title="Mese successivo">{@html ico.chevR}</button>
    </div>
  </div>

  <div class="grid">
    {#each days as d}<span class="dow">{d}</span>{/each}
    {#key title}
      {#each cells as c, i}
        <span class="day" class:out={c.out} class:today={c.today} class:weekend={c.weekend} style="animation-delay:{i * 6}ms">
          {c.d.getDate()}
        </span>
      {/each}
    {/key}
  </div>

  <footer>
    <button class="link" onclick={() => api.launch("ms-settings:dateandtime")}>Impostazioni data e ora</button>
  </footer>
</div>

<style>
  .cal {
    height: 100%;
    display: flex;
    flex-direction: column;
    padding: 20px 18px 12px;
  }
  header {
    margin-bottom: 14px;
  }
  .time {
    font-size: 34px;
    font-weight: 300;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
  }
  .date {
    color: var(--fg-dim);
    text-transform: capitalize;
  }
  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  .nav b {
    text-transform: capitalize;
    font-weight: 600;
  }
  .nav div {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .nb {
    width: 28px;
    height: 28px;
    padding: 6px;
  }
  .today-btn {
    padding: 4px 10px;
    border-radius: 6px;
    font-size: 12px;
  }
  .today-btn:hover {
    background: var(--hover);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 2px;
    text-align: center;
  }
  .dow {
    font-size: 11px;
    color: var(--fg-faint);
    padding: 6px 0;
  }
  .day {
    aspect-ratio: 1;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-variant-numeric: tabular-nums;
    animation: fade 0.3s ease both;
    transition: background 0.15s;
  }
  .day:hover {
    background: var(--hover);
  }
  .day.weekend {
    color: var(--fg-dim);
  }
  .day.out {
    color: var(--fg-faint);
  }
  .day.today {
    background: var(--accent);
    color: #000;
    font-weight: 600;
    box-shadow: 0 0 14px color-mix(in srgb, var(--accent) 60%, transparent);
  }
  @keyframes fade {
    from {
      opacity: 0;
      transform: scale(0.8);
    }
  }
  footer {
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid var(--stroke);
  }
  .link {
    color: var(--accent);
    font-size: 12px;
    padding: 6px 2px;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
