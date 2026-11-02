import { useEffect, useState } from "react";
import { commands, events, type SyncStatus_Serialize } from "../../bindings";
import { formatAge } from "../../lib/format";

/** Sync state in one line: failed > offline > pending > syncing > "Synced 12 s ago". */
export function StatusArea({ onError }: { onError: (message: string) => void }) {
  const [status, setStatus] = useState<SyncStatus_Serialize>();
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    void commands.getSyncStatus().then(setStatus);
    const off = events.syncStatusChanged.listen((e) => setStatus(e.payload.status));
    const tick = window.setInterval(() => setNow(Date.now()), 1000);
    return () => {
      void off.then((f) => f());
      window.clearInterval(tick);
    };
  }, []);

  if (!status) return <div className="status" />;

  const failed = status.failedActions;
  const each = async (fn: typeof commands.retryFailed) => {
    for (const action of failed) {
      const result = await fn(action.id);
      if (result.status === "error") onError(result.error);
    }
  };
  const lastSync = Math.max(0, ...status.accounts.map((a) => a.lastSyncAt ?? 0));
  const states = status.accounts.map((a) => a.state);

  let text: string;
  let tone = "ok";
  if (failed.length > 0) {
    text = `${failed.length} action${failed.length > 1 ? "s" : ""} failed`;
    tone = "bad";
  } else if (states.includes("offline")) {
    text = "Offline";
    tone = "warn";
  } else if (states.includes("authError")) {
    text = "Sign-in required";
    tone = "warn";
  } else if (status.pendingActions > 0) {
    text = `${status.pendingActions} action${status.pendingActions > 1 ? "s" : ""} pending`;
  } else if (states.includes("syncing")) {
    text = "Syncing…";
  } else {
    text = lastSync ? `Synced ${formatAge(now - lastSync)} ago` : "Not synced yet";
  }

  return (
    <div className="status" data-tone={tone}>
      <span className="status-dot" />
      <span className="status-text">{text}</span>
      {failed.length > 0 && (
        <>
          <button onClick={() => void each(commands.retryFailed)}>Retry</button>
          <button onClick={() => void each(commands.dismissFailed)}>Dismiss</button>
        </>
      )}
    </div>
  );
}
