import { commands } from "../bindings";

declare global {
  interface Window {
    __tidemailColdLaunchMs?: number;
  }
}

let reported = false;

/** Shows the hidden window once the first list frame has painted. Rust returns the cold-launch time. */
export function reportFirstPaint() {
  if (reported) return;
  reported = true;
  // The first rAF runs before paint; the second runs after that frame is on screen.
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      commands
        .reportFirstPaint()
        .then((result) => {
          if (result.status !== "ok") return;
          window.__tidemailColdLaunchMs = result.data;
          performance.mark("tidemail:first-paint", { detail: result.data });
        })
        .catch(() => undefined);
    }),
  );
}
