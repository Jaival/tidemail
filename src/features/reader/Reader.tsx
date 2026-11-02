import { useCallback, useEffect, useRef, useState } from "react";
import {
  commands,
  events,
  type Attachment_Serialize,
  type Body_Serialize,
  type MessageMeta_Serialize,
  type ThreadDetail_Serialize,
} from "../../bindings";
import { buildSrcdoc, hasRemoteImages } from "./srcdoc";

/**
 * Reader pane. Contract with the UI shell: the shell renders `<Reader threadId={…} />`
 * in its reader slot; `null` means nothing is selected.
 *
 * One sandboxed iframe stays mounted for the life of the pane; only its `srcdoc` changes.
 * The sandbox has no `allow-same-origin`, so the parent cannot read the iframe's height.
 * The iframe therefore fills the free pane height and scrolls inside. That also means
 * no layout measuring on j/k, which keeps open-to-paint short.
 */
export function Reader({ threadId }: { threadId: number | null }) {
  const [detail, setDetail] = useState<ThreadDetail_Serialize | null>(null);
  const [selected, setSelected] = useState<number | null>(null);
  const [body, setBody] = useState<Body_Serialize | null>(null);
  const [error, setError] = useState<string>();
  const bodies = useRef(new Map<number, Body_Serialize>());

  const loadBody = useCallback(async (messageId: number, force = false) => {
    const cached = force ? undefined : bodies.current.get(messageId);
    if (cached) return cached;
    const res = await commands.getBody(messageId);
    if (res.status === "error") throw new Error(res.error);
    if (res.data) bodies.current.set(messageId, res.data);
    return res.data;
  }, []);

  // Load the thread, then the body of its newest message. The old content stays on
  // screen until the new one is ready, so j/k never flashes an empty pane.
  const load = useCallback(
    async (id: number, isCurrent: () => boolean) => {
      try {
        const res = await commands.getThread(id);
        if (res.status === "error") throw new Error(res.error);
        const thread = res.data;
        const last = thread?.messages.at(-1);
        const b = last ? await loadBody(last.id) : null;
        if (!isCurrent()) return;
        setDetail(thread);
        setSelected((prev) =>
          thread?.messages.some((m) => m.id === prev) ? prev : (last?.id ?? null),
        );
        setBody(b);
        setError(undefined);
      } catch (e) {
        if (isCurrent()) setError(String(e));
      }
    },
    [loadBody],
  );

  useEffect(() => {
    if (threadId === null) {
      setDetail(null);
      setSelected(null);
      setBody(null);
      return;
    }
    let current = true;
    setSelected(null);
    void load(threadId, () => current);
    return () => {
      current = false;
    };
  }, [threadId, load]);

  // Body arrived or thread changed in the background.
  useEffect(() => {
    if (threadId === null) return;
    let current = true;
    const unlisten = events.dataChanged.listen((e) => {
      if (!e.payload.threadIds.includes(threadId)) return;
      bodies.current.clear();
      void load(threadId, () => current);
    });
    return () => {
      current = false;
      void unlisten.then((f) => f());
    };
  }, [threadId, load]);

  const select = async (m: MessageMeta_Serialize) => {
    try {
      setBody(await loadBody(m.id));
      setSelected(m.id);
      setError(undefined);
    } catch (e) {
      setError(String(e));
    }
  };

  const message = detail?.messages.find((m) => m.id === selected);

  const loadImages = async () => {
    if (!message) return;
    const res = await commands.setSenderImages(message.fromEmail, true);
    if (res.status === "error") return setError(res.error);
    try {
      bodies.current.clear();
      setBody(await loadBody(message.id, true));
    } catch (e) {
      setError(String(e));
    }
  };

  const html = body?.html ?? "";
  const showHtml = html !== "";
  const needsImages = showHtml && !body?.loadImages && hasRemoteImages(html);
  const files = message?.attachments.filter((a) => !a.isInline) ?? [];

  return (
    <section className="reader flex h-full min-h-0 flex-col bg-[var(--panel)] text-[var(--text)]">
      {detail && message && (
        <>
          <header className="border-b border-[var(--line)] px-5 py-3">
            <h2 className="m-0 text-base font-semibold">{message.subject || "(no subject)"}</h2>
            <div className="mt-1 text-xs text-[var(--soft)]">
              <strong>{message.fromName || message.fromEmail}</strong>{" "}
              <span className="text-[var(--muted)]">&lt;{message.fromEmail}&gt;</span>
              <span className="text-[var(--muted)]">
                {" "}
                · {new Date(message.internalDate).toLocaleString()}
              </span>
            </div>
            {message.toText && (
              <div className="truncate text-xs text-[var(--muted)]">To: {message.toText}</div>
            )}
          </header>
          {detail.messages.length > 1 && (
            <ol className="m-0 flex max-h-28 list-none flex-col overflow-auto border-b border-[var(--line)] p-0">
              {detail.messages.map((m) => (
                <li key={m.id}>
                  <button
                    className={`flex w-full gap-2 border-0 px-5 py-1 text-left text-xs ${
                      m.id === selected ? "bg-[#6657dd1a] font-semibold" : "bg-transparent"
                    }`}
                    onClick={() => void select(m)}
                  >
                    <span className="w-40 shrink-0 truncate">{m.fromName || m.fromEmail}</span>
                    <span className="truncate text-[var(--muted)]">{m.snippet}</span>
                  </button>
                </li>
              ))}
            </ol>
          )}
          {needsImages && (
            <div className="flex items-center gap-3 border-b border-[var(--line)] px-5 py-1.5 text-xs text-[var(--soft)]">
              Remote images are blocked.
              <button
                className="rounded border border-[var(--line)] bg-transparent px-2 py-0.5"
                onClick={() => void loadImages()}
              >
                Load images from {message.fromEmail}
              </button>
            </div>
          )}
        </>
      )}
      {error && (
        <div role="alert" className="px-5 py-2 text-xs text-red-400">
          {error}
        </div>
      )}
      {detail && !body && !error && (
        <p className="m-0 px-5 py-4 text-xs text-[var(--muted)]">Message body not downloaded yet.</p>
      )}
      {body && !showHtml && (
        <pre className="m-0 min-h-0 flex-1 overflow-auto whitespace-pre-wrap px-5 py-4 font-[inherit] text-sm">
          {body.text}
        </pre>
      )}
      {/* Always mounted (warm); only srcdoc changes. */}
      <iframe
        title="Message"
        sandbox="allow-popups allow-popups-to-escape-sandbox"
        srcDoc={showHtml ? buildSrcdoc(html, body?.loadImages ?? false) : ""}
        className={`min-h-0 w-full flex-1 border-0 bg-white ${showHtml ? "" : "hidden"}`}
      />
      {files.length > 0 && <AttachmentList files={files} />}
    </section>
  );
}

function AttachmentList({ files }: { files: Attachment_Serialize[] }) {
  const [state, setState] = useState<Record<number, string>>({});
  const download = async (a: Attachment_Serialize) => {
    setState((s) => ({ ...s, [a.id]: "Downloading…" }));
    const res = await commands.downloadAttachment(a.id);
    setState((s) => ({
      ...s,
      [a.id]: res.status === "ok" ? `Saved to ${res.data}` : `Failed: ${res.error}`,
    }));
  };
  return (
    <ul className="m-0 max-h-32 list-none overflow-auto border-t border-[var(--line)] px-5 py-2 text-xs">
      {files.map((a) => (
        <li key={a.id} className="flex items-center gap-3 py-0.5">
          <button
            className="rounded border border-[var(--line)] bg-transparent px-2 py-0.5"
            onClick={() => void download(a)}
          >
            {a.filename}
          </button>
          <span className="text-[var(--muted)]">{formatSize(a.size)}</span>
          {state[a.id] && <span className="truncate text-[var(--soft)]">{state[a.id]}</span>}
        </li>
      ))}
    </ul>
  );
}

function formatSize(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
