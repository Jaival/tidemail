import { useEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type Account_Serialize,
  type ActionKind,
  type Label_Serialize,
  type ThreadRow,
} from "../../bindings";
import { reportFirstPaint } from "../../lib/firstPaint";
import { unwrap } from "../../lib/ipc";
import { ThreadList } from "../list/ThreadList";
import { useThreadList } from "../list/useThreadList";
import { Reader } from "../reader/Reader";
import { SearchPanel } from "../search/SearchPanel";
import { useTheme, type ThemePref } from "../theme/useTheme";
import { Palette, type PaletteItem } from "./Palette";
import { Sidebar, type Nav } from "./Sidebar";
import { useShortcuts, type Command } from "./shortcuts";
import { StatusArea } from "./StatusArea";

const TOAST_MS = 5000;

type Toast = { id: number; text: string; error?: boolean; undoIds?: number[] };
type Overlay =
  | { type: "commands" }
  | { type: "labels"; threadId: number; accountId: number; keys: string[] };

const navTo = (labelKey: string, title: string): Nav => ({ view: { labelKey, accountId: null }, title });

/** Three panes plus palette, toast and status. Owns selection, actions and undo. */
export function Shell() {
  const [themePref, setThemePref] = useTheme();
  const [nav, setNav] = useState<Nav>(navTo("INBOX", "Inbox"));
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [toast, setToast] = useState<Toast | null>(null);
  const [accounts, setAccounts] = useState<Account_Serialize[]>([]);
  const [labels, setLabels] = useState<Label_Serialize[]>([]);
  const searchRef = useRef<HTMLInputElement>(null);
  const undoStack = useRef<number[][]>([]);
  const toastId = useRef(0);

  const list = useThreadList(nav.view);
  const { threads } = list;
  const searching = query.trim() !== "";
  const ready = list.loaded;

  const latest = useRef({ nav, list });
  latest.current = { nav, list };

  const notify = (text: string, extra?: Partial<Toast>) => setToast({ id: ++toastId.current, text, ...extra });

  // Everything beyond the first page waits for it, so the first paint is not delayed.
  useEffect(() => {
    if (!ready) return;
    reportFirstPaint();
    const loadLabels = () => void commands.listLabels(null).then(unwrap).then(setLabels);
    void commands.listAccounts().then(unwrap).then(setAccounts);
    loadLabels();
    let timer: number | undefined;
    let listDirty = false;
    // Events can arrive in bursts during backfill: re-query once per burst.
    const off = events.dataChanged.listen((e) => {
      const { nav, list } = latest.current;
      const ids = new Set(list.threads.map((t) => t.id));
      if (e.payload.labelKeys.includes(nav.view.labelKey) || e.payload.threadIds.some((id) => ids.has(id))) {
        listDirty = true;
      }
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (listDirty) latest.current.list.refresh();
        listDirty = false;
        loadLabels();
      }, 100);
    });
    return () => {
      window.clearTimeout(timer);
      void off.then((f) => f());
    };
  }, [ready]);

  useEffect(() => {
    if (!toast) return;
    const timer = window.setTimeout(() => setToast(null), TOAST_MS);
    return () => window.clearTimeout(timer);
  }, [toast]);

  const act = async (kind: ActionKind, ids: number[], text: string) => {
    const removed = list.apply(ids, kind);
    const index = threads.findIndex((t) => t.id === selectedId);
    if (removed && index >= 0 && ids.includes(selectedId!)) {
      setSelectedId(threads[index + 1]?.id ?? threads[index - 1]?.id ?? null);
    }
    const result = await commands.applyAction({ threadIds: ids, kind });
    if (result.status === "error") {
      list.refresh(); // reverts the optimistic change from the database
      notify(`${text} failed: ${result.error}`, { error: true });
      return;
    }
    undoStack.current.push(result.data.actionIds);
    notify(text, { undoIds: result.data.actionIds });
  };

  const undo = async (actionIds: number[]) => {
    undoStack.current = undoStack.current.filter((ids) => ids !== actionIds);
    setToast(null);
    const result = await commands.undoAction(actionIds);
    if (result.status === "error") notify(`Undo failed: ${result.error}`, { error: true });
    else list.refresh();
  };

  /** The selected thread's row; fetched when it was opened from search and is not in the list. */
  const selectedThread = async (): Promise<ThreadRow | null> => {
    if (selectedId === null) return null;
    return threads.find((t) => t.id === selectedId) ?? unwrap(await commands.getThread(selectedId))?.thread ?? null;
  };

  const move = (delta: number) => {
    if (searching) return;
    const i = threads.findIndex((t) => t.id === selectedId);
    const next = threads[Math.min(Math.max(i + delta, 0), threads.length - 1)];
    if (next) setSelectedId(next.id);
  };

  const go = (labelKey: string, title: string) => {
    setNav(navTo(labelKey, title));
    setQuery("");
  };

  const openLabelPicker = async () => {
    if (selectedId === null) return;
    const detail = unwrap(await commands.getThread(selectedId));
    if (detail) {
      setOverlay({ type: "labels", threadId: selectedId, accountId: detail.thread.accountId, keys: detail.labelKeys });
    }
  };

  const themes: [ThemePref, string][] = [
    ["system", "Theme: follow Windows"],
    ["dark", "Theme: dark"],
    ["light", "Theme: light"],
  ];
  const cmds: Command[] = [
    { id: "next", title: "Next thread", shortcut: "j", run: () => move(1) },
    { id: "prev", title: "Previous thread", shortcut: "k", run: () => move(-1) },
    { id: "open", title: "Open thread", shortcut: "Enter", run: () => { if (selectedId === null) move(1); } },
    {
      id: "archive",
      title: "Archive",
      shortcut: "e",
      run: () => { if (selectedId !== null) void act({ type: "archive" }, [selectedId], "Archived"); },
    },
    {
      id: "trash",
      title: "Move to trash",
      shortcut: "#",
      run: () => { if (selectedId !== null) void act({ type: "trash" }, [selectedId], "Moved to trash"); },
    },
    {
      id: "star",
      title: "Star / unstar",
      shortcut: "s",
      run: () =>
        void selectedThread().then(
          (t) =>
            t && act({ type: t.isStarred ? "unstar" : "star" }, [t.id], t.isStarred ? "Unstarred" : "Starred"),
        ),
    },
    { id: "label", title: "Label…", shortcut: "l", run: () => void openLabelPicker() },
    {
      id: "unread",
      title: "Mark unread / read",
      shortcut: "u",
      run: () =>
        void selectedThread().then(
          (t) =>
            t &&
            act(
              { type: t.unreadCount > 0 ? "markRead" : "markUnread" },
              [t.id],
              t.unreadCount > 0 ? "Marked read" : "Marked unread",
            ),
        ),
    },
    {
      id: "undo",
      title: "Undo last action",
      shortcut: "Ctrl+Z",
      run: () => {
        const ids = undoStack.current.pop();
        if (ids) void undo(ids);
      },
    },
    { id: "search", title: "Search", shortcut: "/", run: () => searchRef.current?.focus() },
    { id: "palette", title: "Command palette", shortcut: "Ctrl+K", hidden: true, run: () => setOverlay((o) => (o ? null : { type: "commands" })) },
    { id: "inbox", title: "Go to Inbox", shortcut: "g i", run: () => go("INBOX", "Inbox") },
    { id: "starred", title: "Go to Starred", run: () => go("STARRED", "Starred") },
    { id: "sent", title: "Go to Sent", run: () => go("SENT", "Sent") },
    { id: "drafts", title: "Go to Drafts", run: () => go("DRAFT", "Drafts") },
    ...themes.map(([pref, title]): Command => ({ id: `theme-${pref}`, title, run: () => setThemePref(pref) })),
  ];
  useShortcuts(cmds);

  let paletteProps: { placeholder: string; items: PaletteItem[]; onPick: (id: string) => void } | null = null;
  if (overlay?.type === "commands") {
    paletteProps = {
      placeholder: `Type a command (theme: ${themePref})`,
      items: cmds.filter((c) => !c.hidden).map((c) => ({ id: c.id, title: c.title, hint: c.shortcut })),
      onPick: (id) => {
        setOverlay(null);
        cmds.find((c) => c.id === id)?.run();
      },
    };
  } else if (overlay?.type === "labels") {
    const mine = labels.filter((l) => !l.isSystem && l.accountId === overlay.accountId);
    paletteProps = {
      placeholder: "Toggle label",
      items: mine.map((l) => ({ id: l.labelKey, title: l.name, checked: overlay.keys.includes(l.labelKey) })),
      onPick: (key) => {
        const label = mine.find((l) => l.labelKey === key)!;
        const has = overlay.keys.includes(key);
        setOverlay(null);
        void act(
          has ? { type: "removeLabel", label_key: key } : { type: "addLabel", label_key: key },
          [overlay.threadId],
          has ? `Removed ${label.name}` : `Labeled ${label.name}`,
        );
      },
    };
  }

  const colors = new Map(accounts.map((a) => [a.id, a.color]));

  return (
    <main>
      <Sidebar accounts={accounts} labels={labels} current={nav} onNavigate={(n) => { setNav(n); setQuery(""); }}>
        {ready && <StatusArea onError={(text) => notify(text, { error: true })} />}
      </Sidebar>
      <section className="workspace">
        <header className="topbar">
          <h1>{nav.title}</h1>
          <label className="search">
            <span>⌕</span>
            <input
              ref={searchRef}
              aria-label="Search mail"
              placeholder="Search mail"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Escape") return;
                setQuery("");
                e.currentTarget.blur();
              }}
            />
            <kbd>/</kbd>
          </label>
        </header>
        <div className="panes">
          <div className="list-pane">
            {searching ? (
              <SearchPanel query={query} onOpenThread={setSelectedId} />
            ) : (
              <ThreadList
                threads={threads}
                loaded={list.loaded}
                error={list.error}
                selectedId={selectedId}
                colors={colors}
                showDots={nav.view.accountId == null}
                onSelect={setSelectedId}
                onNearEnd={list.loadMore}
              />
            )}
          </div>
          <div className="reader-pane">
            <Reader threadId={selectedId} />
          </div>
        </div>
      </section>
      {toast && (
        <div className="toast" role="status" data-error={toast.error ?? false}>
          <span>{toast.text}</span>
          {toast.undoIds && <button onClick={() => void undo(toast.undoIds!)}>Undo</button>}
        </div>
      )}
      {paletteProps && <Palette key={overlay!.type} {...paletteProps} onClose={() => setOverlay(null)} />}
    </main>
  );
}
