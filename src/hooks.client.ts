// Forward frontend errors to the Rust console (visible in `tauri dev`).
import { invoke } from "@tauri-apps/api/core";

const isTauri = "__TAURI_INTERNALS__" in window;
export function report(msg: string) {
  if (isTauri) invoke("log_js", { msg }).catch(() => {});
}

window.addEventListener("error", (e) => report(`error: ${e.message} @ ${e.filename}:${e.lineno}`));
window.addEventListener("unhandledrejection", (e) => report(`rejection: ${String(e.reason)}`));
report(`boot ${location.pathname} ${innerWidth}x${innerHeight} visible=${document.visibilityState}`);
document.addEventListener("visibilitychange", () => report(`visibility=${document.visibilityState}`));

export const handleError = ({ error }: { error: unknown }) => {
  report(`sveltekit: ${String((error as Error)?.stack ?? error)}`);
};
