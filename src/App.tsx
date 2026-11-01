import { useEffect, useRef, useState } from "react";
import { commands } from "./bindings";
import { useVirtualizer } from "@tanstack/react-virtual";

type ThreadRow = { id: number; subject: string; snippet: string; participants: string; unreadCount: number; lastDate: number };

export function App() {
  const [threads, setThreads] = useState<ThreadRow[]>([]);
  const [error, setError] = useState<string>();
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const listRef = useRef<HTMLDivElement>(null);
  const rowVirtualizer = useVirtualizer({
    count: threads.length,
    getScrollElement: () => listRef.current,
    estimateSize: () => 56,
    overscan: 10,
  });

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => setTheme(media.matches ? "dark" : "light");
    update();
    media.addEventListener("change", update);
    const started = Date.now();
    commands.listThreads({ labelKey: "INBOX", accountId: null }, null, 60)
      .then((result) => {
        if (result.status === "ok") setThreads(result.data.items);
        else setError(result.error);
      })
      .catch((e) => setError(String(e)))
      .finally(() => requestAnimationFrame(() => { void commands.reportFirstPaint(started).catch(() => undefined); }));
    return () => media.removeEventListener("change", update);
  }, []);

  return <main data-theme={theme}>
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark">t</span><span>Tidemail</span></div>
      <button className="compose" disabled>Compose <kbd>C</kbd></button>
      <nav aria-label="Mailbox">
        <button className="nav-item active"><span className="nav-icon">▣</span>Inbox</button>
        <button className="nav-item"><span className="nav-icon">☆</span>Starred</button>
        <button className="nav-item"><span className="nav-icon">↗</span>Sent</button>
        <button className="nav-item"><span className="nav-icon">◇</span>Drafts</button>
      </nav>
      <div className="sidebar-bottom"><span className="status-dot" />Local mailbox</div>
    </aside>
    <section className="workspace">
      <header className="topbar">
        <div className="view-title"><span className="eyebrow">MAILBOX</span><h1>Inbox</h1></div>
        <label className="search"><span>⌕</span><input aria-label="Search mail" placeholder="Search mail" /><kbd>/</kbd></label>
        <button className="avatar" aria-label="Accounts">J</button>
      </header>
      <section className="mail-panel">
        <div className="list-toolbar"><div><strong>All mail</strong><span className="count">{threads.length ? `${threads.length} recent` : ""}</span></div><div className="toolbar-actions"><button aria-label="Refresh">↻</button><button aria-label="More options">···</button></div></div>
        {error ? <div className="empty"><span className="empty-icon">!</span><h2>Could not open mailbox</h2><p>{error}</p></div> : threads.length === 0 ? <div className="empty"><span className="empty-icon">✳</span><h2>Your inbox is clear</h2><p>Messages will appear here after sync starts.</p></div> : <div className="message-list" ref={listRef}><div className="virtual-space" style={{ height: rowVirtualizer.getTotalSize() }}>{rowVirtualizer.getVirtualItems().map((row) => {
          const thread = threads[row.index];
          return <button className="message-row" key={thread.id} style={{ transform: `translateY(${row.start}px)` }}><span className="sender">{thread.participants || "Unknown sender"}</span><span className="message-copy"><strong>{thread.subject || "(no subject)"}</strong><span> — {thread.snippet}</span></span><time>{new Date(thread.lastDate).toLocaleDateString()}</time></button>;
        })}</div></div>}
      </section>
    </section>
  </main>;
}
