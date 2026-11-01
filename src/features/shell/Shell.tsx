import type { ReactNode } from "react";

/** Sidebar, top bar and the main panel. Phase 1 (UI shell) adds navigation, palette and status. */
export function Shell({ children }: { children: ReactNode }) {
  return (
    <main>
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">t</span>
          <span>Tidemail</span>
        </div>
        <button className="compose" disabled>
          Compose <kbd>C</kbd>
        </button>
        <nav aria-label="Mailbox">
          <button className="nav-item active"><span className="nav-icon">▣</span>Inbox</button>
          <button className="nav-item"><span className="nav-icon">☆</span>Starred</button>
          <button className="nav-item"><span className="nav-icon">↗</span>Sent</button>
          <button className="nav-item"><span className="nav-icon">◇</span>Drafts</button>
        </nav>
        <div className="sidebar-bottom">
          <span className="status-dot" />
          Local mailbox
        </div>
      </aside>
      <section className="workspace">
        <header className="topbar">
          <div className="view-title">
            <span className="eyebrow">MAILBOX</span>
            <h1>Inbox</h1>
          </div>
          <label className="search">
            <span>⌕</span>
            <input aria-label="Search mail" placeholder="Search mail" />
            <kbd>/</kbd>
          </label>
          <button className="avatar" aria-label="Accounts">J</button>
        </header>
        <section className="mail-panel">
          <div className="list-toolbar">
            <div><strong>All mail</strong></div>
            <div className="toolbar-actions">
              <button aria-label="Refresh">↻</button>
              <button aria-label="More options">···</button>
            </div>
          </div>
          {children}
        </section>
      </section>
    </main>
  );
}
