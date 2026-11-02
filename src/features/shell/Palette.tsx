import { useMemo, useRef, useState } from "react";

export type PaletteItem = { id: string; title: string; hint?: string; checked?: boolean };

const MAX_ITEMS = 100;

/** Filterable list opened by Ctrl+K (commands) and `l` (labels). */
export function Palette({
  placeholder,
  items,
  onPick,
  onClose,
}: {
  placeholder: string;
  items: PaletteItem[];
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const shown = useMemo(() => {
    const q = text.trim().toLowerCase();
    return items.filter((i) => i.title.toLowerCase().includes(q)).slice(0, MAX_ITEMS);
  }, [items, text]);
  const current = Math.min(active, shown.length - 1);

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "Enter" && shown[current]) onPick(shown[current].id);
    else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      const next = Math.min(Math.max(current + (e.key === "ArrowDown" ? 1 : -1), 0), shown.length - 1);
      setActive(next);
      listRef.current?.children[next]?.scrollIntoView({ block: "nearest" });
    } else return;
    e.preventDefault();
  };

  return (
    <div className="palette-backdrop" onMouseDown={onClose}>
      <div className="palette" role="dialog" aria-label={placeholder} onMouseDown={(e) => e.stopPropagation()}>
        <input
          autoFocus
          value={text}
          placeholder={placeholder}
          onChange={(e) => {
            setText(e.target.value);
            setActive(0);
          }}
          onKeyDown={onKeyDown}
        />
        <ul ref={listRef}>
          {shown.map((item, i) => (
            <li
              key={item.id}
              data-active={i === current}
              onMouseMove={() => setActive(i)}
              onClick={() => onPick(item.id)}
            >
              <span>
                {item.checked !== undefined && (item.checked ? "✓ " : " ")}
                {item.title}
              </span>
              {item.hint && <kbd>{item.hint}</kbd>}
            </li>
          ))}
          {shown.length === 0 && <li className="palette-empty">No matches</li>}
        </ul>
      </div>
    </div>
  );
}
