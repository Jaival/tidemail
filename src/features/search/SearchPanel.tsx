/**
 * Search results. Contract with the UI shell: the shell renders `<SearchPanel … />` while search
 * is active (`/` focuses its input), and `onOpenThread` selects a thread in the reader.
 *
 * One `commands.search` call per query change; a response is dropped if a newer query was sent
 * meanwhile. Up/Down/Enter work from anywhere while the panel is mounted (the input keeps focus).
 */
import { useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { commands, type SearchHit } from "../../bindings";
import { parseSnippet } from "./snippet";

const LIMIT = 50;
const ROW_HEIGHT = 64;

export function SearchPanel({
  query,
  onOpenThread,
}: {
  query: string;
  onOpenThread: (threadId: number) => void;
}) {
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [error, setError] = useState<string>();
  const [selected, setSelected] = useState(0);
  const latest = useRef(0);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const seq = ++latest.current;
    if (!query.trim()) {
      setHits([]);
      setError(undefined);
      return;
    }
    commands
      .search(query, LIMIT)
      .then((result) => {
        if (seq !== latest.current) return;
        if (result.status === "ok") {
          setHits(result.data);
          setError(undefined);
          setSelected(0);
        } else {
          setError(result.error);
        }
      })
      .catch((e: unknown) => {
        if (seq === latest.current) setError(String(e));
      });
  }, [query]);

  const virtualizer = useVirtualizer({
    count: hits.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 8,
  });

  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.ctrlKey || e.metaKey || e.altKey || hits.length === 0) return;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const next = Math.min(hits.length - 1, Math.max(0, selected + (e.key === "ArrowDown" ? 1 : -1)));
        setSelected(next);
        virtualizer.scrollToIndex(next);
      } else if (e.key === "Enter") {
        e.preventDefault();
        onOpenThread(hits[selected].thread.id);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [hits, selected, onOpenThread, virtualizer]);

  if (error) {
    return <p className="p-6 text-xs text-[var(--muted)]">Search failed: {error}</p>;
  }
  if (hits.length === 0) {
    return (
      <p className="p-6 text-xs text-[var(--muted)]">
        {query.trim() ? "No matching conversations." : "Type to search. Try from:, to:, label:, has:attachment, is:unread, before:, after:."}
      </p>
    );
  }
  return (
    <section ref={scrollRef} className="h-[calc(100vh-205px)] min-h-[300px] overflow-auto" aria-label="Search results">
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((row) => {
          const hit = hits[row.index];
          const t = hit.thread;
          return (
            <button
              key={t.id}
              type="button"
              onClick={() => {
                setSelected(row.index);
                onOpenThread(t.id);
              }}
              className={`absolute left-0 top-0 flex h-16 w-full flex-col justify-center gap-0.5 border-0 border-b border-[var(--line)] px-[18px] text-left text-xs text-[var(--text)] hover:bg-[var(--line)] ${
                row.index === selected ? "bg-[var(--line)]" : "bg-transparent"
              }`}
              style={{ transform: `translateY(${row.start}px)` }}
            >
              <span className="flex items-baseline gap-4">
                <span className={`w-44 shrink-0 truncate ${t.unreadCount > 0 ? "font-bold" : "font-medium"}`}>
                  {t.participants || "Unknown sender"}
                </span>
                <span className="min-w-0 flex-1 truncate">{t.subject || "(no subject)"}</span>
                <time className="shrink-0 text-[10px] text-[var(--muted)]">
                  {new Date(t.lastDate).toLocaleDateString()}
                </time>
              </span>
              <span className="truncate text-[var(--muted)]">
                {parseSnippet(hit.snippetHtml).map((part, i) =>
                  part.match ? (
                    <mark key={i} className="rounded-sm bg-[#8d80ff55] px-px text-inherit">
                      {part.text}
                    </mark>
                  ) : (
                    <span key={i}>{part.text}</span>
                  ),
                )}
              </span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
