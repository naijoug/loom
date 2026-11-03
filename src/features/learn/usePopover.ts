import { useEffect, useRef, useState, type KeyboardEvent } from "react";

/** Open state for a trigger + popup pair wrapped in `ref`: closes on outside press and on Escape (refocusing the trigger). */
export function usePopover<T extends HTMLElement = HTMLSpanElement>() {
  const [open, setOpen] = useState(false);
  const ref = useRef<T>(null);
  useEffect(() => {
    if (!open) return;
    const onPress = (event: MouseEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKey = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      // Handled here first (capture), so an enclosing drawer stays open.
      event.preventDefault();
      setOpen(false);
      ref.current?.querySelector<HTMLElement>("[aria-expanded]")?.focus();
    };
    document.addEventListener("mousedown", onPress);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("mousedown", onPress);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open]);
  return { open, setOpen, ref };
}

/** Menu keyboard support: focus the checked (or first) item on open, ArrowUp / ArrowDown move with wrap-around. */
export function useMenuKeys(open: boolean) {
  const list = useRef<HTMLDivElement>(null);
  const items = () => [...(list.current?.querySelectorAll<HTMLElement>("[role^=menuitem]:not(:disabled)") ?? [])];
  useEffect(() => {
    if (!open) return;
    const all = items();
    (all.find((item) => item.matches('[role="menuitemradio"][aria-checked="true"]')) ?? all[0])?.focus();
  }, [open]);
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const all = items();
    const at = all.indexOf(document.activeElement as HTMLElement);
    all[(at + (event.key === "ArrowDown" ? 1 : all.length - 1)) % all.length]?.focus();
  };
  return { list, onKeyDown };
}
