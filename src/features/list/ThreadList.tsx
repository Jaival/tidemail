import { useEffect, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { ThreadRow } from "../../bindings";
import { formatDate } from "../../lib/format";

const ROW_HEIGHT = 64;

type Props = {
  threads: ThreadRow[];
  loaded: boolean;
  error?: string;
  selectedId: number | null;
  /** Account colors by id; dots show only when `showDots` (unified views). */
  colors: Map<number, string>;
  showDots: boolean;
  onSelect: (threadId: number) => void;
  onNearEnd: () => void;
};

export function ThreadList({ threads, loaded, error, selectedId, colors, showDots, onSelect, onNearEnd }: Props) {
  const listRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: threads.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 10,
    // Paint rows in the first commit instead of waiting for the first measurement.
    initialRect: { width: 400, height: window.innerHeight },
  });

  const selectedIndex = selectedId === null ? -1 : threads.findIndex((t) => t.id === selectedId);
  useEffect(() => {
    if (selectedIndex >= 0) virtualizer.scrollToIndex(selectedIndex);
  }, [selectedIndex, virtualizer]);

  const items = virtualizer.getVirtualItems();
  const lastIndex = items.length ? items[items.length - 1].index : -1;
  useEffect(() => {
    // Load the next page well before the user reaches the end.
    if (lastIndex >= threads.length - 30) onNearEnd();
  }, [lastIndex, threads.length, onNearEnd]);

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
        <h2>Nothing here</h2>
        <p>Messages will appear here after sync starts.</p>
      </div>
    );
  }
  return (
    <div className="message-list" ref={listRef} role="listbox" aria-label="Threads">
      <div className="virtual-space" style={{ height: virtualizer.getTotalSize() }}>
        {items.map((row) => {
          const thread = threads[row.index];
          return (
            <div
              className="message-row"
              role="option"
              aria-selected={thread.id === selectedId}
              data-unread={thread.unreadCount > 0}
              key={thread.id}
              data-index={row.index}
              style={{ transform: `translateY(${row.start}px)` }}
              onClick={() => onSelect(thread.id)}
            >
              <span
                className="account-dot"
                style={showDots ? { background: colors.get(thread.accountId) } : undefined}
              />
              <span className="row-main">
                <span className="row-top">
                  <span className="sender">{thread.participants || "Unknown sender"}</span>
                  {thread.messageCount > 1 && <span className="count">{thread.messageCount}</span>}
                  <time>{formatDate(thread.lastDate)}</time>
                </span>
                <span className="row-sub">
                  <strong>{thread.subject || "(no subject)"}</strong>
                  <span> — {thread.snippet}</span>
                </span>
              </span>
              {thread.isStarred && (
                <span className="star" aria-label="Starred">
                  ★
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}
