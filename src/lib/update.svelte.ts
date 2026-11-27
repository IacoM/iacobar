// Update state shared by the components of one window (bar or flyout).
// The Rust side checks GitHub in the background and emits `update-available`.

import { api, listen, type UpdateInfo, type UpdateProgress } from "./api";

export const update = $state({
  /** version running now ("" until loaded) */
  current: "",
  available: null as UpdateInfo | null,
  checking: false,
  /** true once a manual check found nothing */
  upToDate: false,
  installing: false,
  /** 0..1 while downloading, null if the size is unknown */
  progress: null as number | null,
  error: "",
});

let started = false;

/** Loads the state once per window and keeps it in sync. */
export function initUpdate() {
  if (started) return;
  started = true;
  api.updateStatus().then((s) => {
    update.current = s.current;
    update.available = s.available;
  });
  listen<UpdateInfo>("update-available", (u) => {
    update.available = u;
    update.upToDate = false;
  });
  listen<UpdateProgress>("update-progress", (p) => {
    update.progress = p.total ? Math.min(1, p.done / p.total) : null;
  });
}

export async function checkNow() {
  update.checking = true;
  update.error = "";
  update.upToDate = false;
  try {
    const s = await api.checkUpdate();
    update.current = s.current;
    update.available = s.available;
    update.upToDate = !s.available;
  } catch (e) {
    update.error = `Controllo non riuscito: ${e}`;
  } finally {
    update.checking = false;
  }
}

export async function installNow() {
  update.installing = true;
  update.error = "";
  update.progress = 0;
  try {
    await api.installUpdate(); // on success the app closes and the installer takes over
  } catch (e) {
    update.error = `Aggiornamento non riuscito: ${e}`;
  } finally {
    update.installing = false;
    update.progress = null;
  }
}
