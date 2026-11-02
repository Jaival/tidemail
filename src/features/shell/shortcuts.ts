import { useEffect, useRef } from "react";

export type Command = {
  id: string;
  title: string;
  /** "j", "#", "Ctrl+K" or a two-key sequence such as "g i". */
  shortcut?: string;
  /** Matched by the keyboard but not listed in the palette. */
  hidden?: boolean;
  run: () => void;
};

const SEQUENCE_MS = 1000;

/** Global keyboard handler. Ignores typing in inputs, except Ctrl+K. */
export function useShortcuts(commands: Command[]) {
  const latest = useRef(commands);
  latest.current = commands;
  useEffect(() => {
    let pending: string | null = null;
    let timer: number | undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.altKey || e.key === "Control" || e.key === "Shift" || e.key === "Meta") return;
      const chord = e.ctrlKey || e.metaKey ? `Ctrl+${e.key.toUpperCase()}` : e.key;
      const t = e.target;
      const typing = t instanceof HTMLElement && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (typing && chord !== "Ctrl+K") return;
      const find = (shortcut: string) => latest.current.find((c) => c.shortcut === shortcut);
      const run = (c: Command) => {
        e.preventDefault();
        c.run();
      };
      if (pending) {
        const sequence = find(`${pending} ${chord}`);
        window.clearTimeout(timer);
        pending = null;
        if (sequence) return run(sequence);
      }
      if (latest.current.some((c) => c.shortcut?.startsWith(`${chord} `))) {
        e.preventDefault();
        pending = chord;
        timer = window.setTimeout(() => (pending = null), SEQUENCE_MS);
        return;
      }
      const single = find(chord);
      if (single) run(single);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
}
