import { useCallback, useEffect, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { commands, type Cursor, type ThreadRow, type View } from "../../bindings";

const PAGE = 100;
const ROW_HEIGHT = 56;

type Props = { view: View; onFirstPage: () => void };

export function ThreadList({ view, onFirstPage }: Props) {
  const [threads, setThreads] = useState<ThreadRow[]>([]);
  const [error, setError] = useState<string>();
  const [loaded, setLoaded] = useState(false);
  const next = useRef<Cursor | null>(null);
  const loading = useRef(false);
  const listRef = useRef<HTMLDivElement>(null);

  const loadPage = useCallback(
    async (cursor: Cursor | null) => {
      if (loading.current) return;
      loading.current = true;
      try {
        const result = await commands.listThreads(view, cursor, PAGE);
        if (result.status === "ok") {
          next.current = result.data.next ?? null;
          setThreads((prev) => (cursor ? [...prev, ...result.data.items] : result.data.items));
        } else {
          setError(result.error);
        }
      } catch (e) {
        setError(String(e));
      } finally {
        loading.current = false;
        setLoaded(true);
      }
    },
    [view],
  );

  useEffect(() => {
    void loadPage(null);
  }, [loadPage]);
  useEffect(() => {
    if (loaded) onFirstPage();
  }, [loaded, onFirstPage]);

  const rowVirtualizer = useVirtualizer({
    count: threads.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
  });

  const items = rowVirtualizer.getVirtualItems();
  const lastIndex = items.length ? items[items.length - 1].index : -1;
  useEffect(() => {
    // Load the next page well before the user reaches the end.
    if (next.current && lastIndex >= threads.length - 30) void loadPage(next.current);
  }, [lastIndex, threads.length, loadPage]);

  if (error) {
    return (
      <div className="empty">
        <span className="empty-icon">!</span>
        <h2>Could not open mailbox</h2>
        <p>{error}</p>
      </div>
    );
  }
  if (loaded && threads.length === 0) {
    return (
      <div className="empty">
        <span className="empty-icon">✳</span>
        <h2>Your inbox is clear</h2>
        <p>Messages will appear here after sync starts.</p>
      </div>
    );
  }
  return (
    <div className="message-list" ref={listRef}>
      <div className="virtual-space" style={{ height: rowVirtualizer.getTotalSize() }}>
        {items.map((row) => {
          const thread = threads[row.index];
          return (
            <button
              className="message-row"
              key={thread.id}
              data-index={row.index}
              style={{ transform: `translateY(${row.start}px)` }}
            >
              <span className="sender">{thread.participants || "Unknown sender"}</span>
              <span className="message-copy">
                <strong>{thread.subject || "(no subject)"}</strong>
                <span> — {thread.snippet}</span>
              </span>
              <time>{new Date(thread.lastDate).toLocaleDateString()}</time>
            </button>
          );
        })}
      </div>
    </div>
  );
}
