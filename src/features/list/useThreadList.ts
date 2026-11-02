import { useCallback, useEffect, useRef, useState } from "react";
import { commands, type ActionKind, type Cursor, type ThreadRow, type View } from "../../bindings";

const PAGE = 100;

/**
 * Keyset-paged thread list for one view. `refresh` re-queries as many rows as are loaded;
 * `apply` is the optimistic local change for an action (returns true when rows left the list).
 */
export function useThreadList(view: View) {
  const [state, setState] = useState<{ threads: ThreadRow[]; loaded: boolean; error?: string }>({
    threads: [],
    loaded: false,
  });
  const threadsRef = useRef(state.threads);
  threadsRef.current = state.threads;
  const next = useRef<Cursor | null>(null);
  const loading = useRef(false);
  // Bumped on every reset so late responses from an older query are dropped.
  const generation = useRef(0);

  const reload = useCallback(
    async (minRows: number) => {
      const gen = ++generation.current;
      loading.current = true;
      let cursor: Cursor | null = null;
      const items: ThreadRow[] = [];
      let error: string | undefined;
      try {
        do {
          const result = await commands.listThreads(view, cursor, PAGE);
          if (gen !== generation.current) return;
          if (result.status === "error") {
            error = result.error;
            break;
          }
          items.push(...result.data.items);
          cursor = result.data.next ?? null;
        } while (cursor && items.length < minRows);
      } finally {
        if (gen === generation.current) loading.current = false;
      }
      next.current = cursor;
      setState(error ? { threads: threadsRef.current, loaded: true, error } : { threads: items, loaded: true });
    },
    [view],
  );

  useEffect(() => {
    void reload(1);
  }, [reload]);

  const refresh = useCallback(() => reload(Math.max(threadsRef.current.length, 1)), [reload]);

  const loadMore = useCallback(async () => {
    if (loading.current || !next.current) return;
    const gen = generation.current;
    loading.current = true;
    try {
      const result = await commands.listThreads(view, next.current, PAGE);
      if (gen !== generation.current || result.status === "error") return;
      next.current = result.data.next ?? null;
      setState((s) => ({ ...s, threads: [...s.threads, ...result.data.items] }));
    } finally {
      if (gen === generation.current) loading.current = false;
    }
  }, [view]);

  const apply = useCallback(
    (ids: number[], kind: ActionKind): boolean => {
      const set = new Set(ids);
      const removes =
        kind.type === "trash" ||
        ((kind.type === "archive" || kind.type === "move") && view.labelKey === "INBOX") ||
        (kind.type === "unstar" && view.labelKey === "STARRED") ||
        (kind.type === "removeLabel" && kind.label_key === view.labelKey);
      const patch = (t: ThreadRow): ThreadRow => {
        switch (kind.type) {
          case "star":
            return { ...t, isStarred: true };
          case "unstar":
            return { ...t, isStarred: false };
          case "markRead":
            return { ...t, unreadCount: 0 };
          case "markUnread":
            return { ...t, unreadCount: Math.max(t.unreadCount, 1) };
          default:
            return t;
        }
      };
      setState((s) => ({
        ...s,
        threads: removes
          ? s.threads.filter((t) => !set.has(t.id))
          : s.threads.map((t) => (set.has(t.id) ? patch(t) : t)),
      }));
      return removes;
    },
    [view],
  );

  return { ...state, loadMore, refresh, apply };
}
