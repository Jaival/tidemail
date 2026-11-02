import type { ReactNode } from "react";
import type { Account_Serialize, Label_Serialize, View } from "../../bindings";

export type Nav = { view: View; title: string };

const MAIN: { key: string; title: string; icon: string }[] = [
  { key: "INBOX", title: "Inbox", icon: "▣" },
  { key: "STARRED", title: "Starred", icon: "☆" },
  { key: "SENT", title: "Sent", icon: "↗" },
  { key: "DRAFT", title: "Drafts", icon: "◇" },
];

const same = (a: View, b: View) => a.labelKey === b.labelKey && (a.accountId ?? null) === (b.accountId ?? null);

export function Sidebar({
  accounts,
  labels,
  current,
  onNavigate,
  children,
}: {
  accounts: Account_Serialize[];
  labels: Label_Serialize[];
  current: Nav;
  onNavigate: (nav: Nav) => void;
  children: ReactNode;
}) {
  const item = (view: View, title: string, lead: ReactNode, className = "") => (
    <button
      key={`${view.labelKey}/${view.accountId ?? "all"}`}
      className={`nav-item ${className} ${same(current.view, view) ? "active" : ""}`}
      onClick={() => onNavigate({ view, title })}
    >
      <span className="nav-icon">{lead}</span>
      <span className="nav-label">{title}</span>
    </button>
  );
  const dot = (color: string) => <span className="account-dot on" style={{ background: color }} />;
  const colorOf = (accountId: number) => accounts.find((a) => a.id === accountId)?.color ?? "var(--muted)";
  const userLabels = labels.filter((l) => !l.isSystem);

  return (
    <aside className="sidebar">
      <div className="brand">
        <span className="brand-mark">t</span>
        <span>Tidemail</span>
      </div>
      <button className="compose" disabled title="Compose arrives in Phase 3">
        Compose <kbd>C</kbd>
      </button>
      <div className="sidebar-scroll">
        <nav aria-label="Mailbox">{MAIN.map((m) => item({ labelKey: m.key, accountId: null }, m.title, m.icon))}</nav>
        {accounts.length > 0 && <h3>Accounts</h3>}
        <nav aria-label="Accounts">
          {accounts.map((a) => (
            <div key={a.id}>
              {item({ labelKey: "INBOX", accountId: a.id }, a.displayName || a.email, dot(a.color))}
              {current.view.accountId === a.id &&
                MAIN.slice(1).map((m) => item({ labelKey: m.key, accountId: a.id }, m.title, m.icon, "sub"))}
            </div>
          ))}
        </nav>
        {userLabels.length > 0 && <h3>Labels</h3>}
        <nav aria-label="Labels">
          {userLabels.map((l) => item({ labelKey: l.labelKey, accountId: l.accountId }, l.name, dot(colorOf(l.accountId))))}
        </nav>
      </div>
      <div className="sidebar-bottom">{children}</div>
    </aside>
  );
}
