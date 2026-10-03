import { createContext, useCallback, useContext, useEffect, useId, useRef, useState, type ReactNode } from "react";
import { AlertCircle, Loader2, X } from "lucide-react";
import type { Category, Member } from "@tendly/contracts";
import { categoryMeta } from "../lib/categories";
import { Mascot, type MascotPose } from "./Mascot";
import { dismissTip, prefersReducedMotion, usePrefs } from "../lib/prefs";

/* ---------------------------------------------------------------- dialog */

type DialogProps = {
  open: boolean;
  onClose: () => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
};

/** Accessible modal built on <dialog>: focus is trapped by the browser,
 * Escape closes it, and focus returns to the element that opened it. */
export function Dialog({ open, onClose, title, children, footer, wide }: DialogProps) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  const opener = useRef<Element | null>(null);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      opener.current = document.activeElement;
      if (typeof d.showModal === "function") d.showModal();
      else d.setAttribute("open", "");
      // showModal() focuses the first focusable element (the close button).
      // Prefer an explicitly marked control, then the first form field.
      const target = d.querySelector<HTMLElement>("[data-autofocus]") ?? d.querySelector<HTMLElement>(".dialog-body input:not([type=hidden]):not([type=checkbox]):not([type=radio]), .dialog-body textarea, .dialog-body select");
      target?.focus();
    } else if (!open && d.open) {
      if (typeof d.close === "function") d.close();
      else d.removeAttribute("open");
      (opener.current as HTMLElement | null)?.focus?.();
    }
  }, [open]);
  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    const onCancel = (e: Event) => {
      e.preventDefault();
      onClose();
    };
    d.addEventListener("cancel", onCancel);
    return () => d.removeEventListener("cancel", onCancel);
  }, [onClose]);
  return (
    <dialog ref={ref} className="dialog" aria-labelledby={titleId} style={wide ? { width: "min(880px, calc(100vw - 24px))" } : undefined}>
      {open && (
        <div className="dialog-inner">
          <div className="dialog-head">
            <h2 id={titleId}>{title}</h2>
            <button type="button" className="btn btn-ghost btn-icon" onClick={onClose} aria-label="Close">
              <X size={20} aria-hidden />
            </button>
          </div>
          <div className="dialog-body">{children}</div>
          {footer && <div className="dialog-foot">{footer}</div>}
        </div>
      )}
    </dialog>
  );
}

/* ---------------------------------------------------------------- form field */

type FieldProps = {
  label: string;
  hint?: string;
  error?: string | null;
  children: (ids: { id: string; describedBy?: string; invalid: boolean }) => ReactNode;
};

export function Field({ label, hint, error, children }: FieldProps) {
  const id = useId();
  const hintId = hint ? `${id}-hint` : undefined;
  const errId = error ? `${id}-err` : undefined;
  const describedBy = [hintId, errId].filter(Boolean).join(" ") || undefined;
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {children({ id, describedBy, invalid: !!error })}
      {hint && (
        <span id={hintId} className="hint">
          {hint}
        </span>
      )}
      {error && (
        <span id={errId} className="error" role="alert">
          {error}
        </span>
      )}
    </div>
  );
}

/* ---------------------------------------------------------------- states */

export function Loading({ label = "Loading…" }: { label?: string }) {
  return (
    <div className="loading" role="status">
      <Loader2 size={18} aria-hidden className={prefersReducedMotion() ? undefined : "spin"} />
      <span>{label}</span>
    </div>
  );
}

export function SkeletonList({ rows = 3 }: { rows?: number }) {
  return (
    <div className="stack-sm" aria-hidden>
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="skeleton" />
      ))}
    </div>
  );
}

export function ErrorState({ message, onRetry }: { message: string; onRetry?: () => void }) {
  return (
    <div className="banner banner-danger" role="alert">
      <AlertCircle size={20} aria-hidden />
      <span style={{ flex: 1 }}>{message}</span>
      {onRetry && (
        <button type="button" className="btn btn-sm" onClick={onRetry}>
          Try again
        </button>
      )}
    </div>
  );
}

export function EmptyState({ title, children, pose = "happy", action }: { title: string; children?: ReactNode; pose?: MascotPose; action?: ReactNode }) {
  const prefs = usePrefs();
  return (
    <div className="empty">
      {!prefs.quiet && <Mascot pose={pose} size={84} animated={!prefersReducedMotion()} />}
      <h3>{title}</h3>
      {children && <div>{children}</div>}
      {action}
    </div>
  );
}

/** A dismissible tip from Pim. Hidden entirely in quiet mode. */
export function Tip({ id, children, pose = "wave" }: { id: string; children: ReactNode; pose?: MascotPose }) {
  const prefs = usePrefs();
  if (prefs.quiet || prefs.dismissedTips.includes(id)) return null;
  return (
    <aside className="tip" aria-label="Tip from Pim">
      <Mascot pose={pose} size={56} animated={!prefersReducedMotion()} />
      <div>{children}</div>
      <button type="button" className="btn btn-ghost btn-icon" onClick={() => dismissTip(id)} aria-label="Dismiss tip">
        <X size={18} aria-hidden />
      </button>
    </aside>
  );
}

/* ---------------------------------------------------------------- badges */

export function CategoryBadge({ category }: { category: Category | null | undefined }) {
  const m = categoryMeta(category);
  if (!m) return null;
  const Icon = m.icon;
  return (
    <span className={`badge cat cat-${m.key}`}>
      <Icon size={14} aria-hidden />
      {m.label}
    </span>
  );
}

export function Avatar({ member, size = 28 }: { member: Member | undefined; size?: number }) {
  if (!member) return null;
  const initials = member.displayName
    .split(/\s+/)
    .map((p) => p[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
  return (
    <span className={`avatar avatar-${member.color}`} style={{ width: size, height: size }} title={member.displayName} aria-hidden>
      {initials}
    </span>
  );
}

/* ---------------------------------------------------------------- toasts */

type Toast = { id: number; text: string; action?: { label: string; run: () => void } };
type ToastApi = { show: (text: string, action?: Toast["action"]) => void };
const ToastCtx = createContext<ToastApi>({ show: () => {} });

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const show = useCallback((text: string, action?: Toast["action"]) => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-2), { id, text, action }]);
    window.setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), action ? 8000 : 4500);
  }, []);
  return (
    <ToastCtx.Provider value={{ show }}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {toasts.map((t) => (
          <div key={t.id} className="toast">
            <span style={{ flex: 1 }}>{t.text}</span>
            {t.action && (
              <button
                type="button"
                className="btn btn-sm"
                onClick={() => {
                  t.action!.run();
                  setToasts((all) => all.filter((x) => x.id !== t.id));
                }}
              >
                {t.action.label}
              </button>
            )}
          </div>
        ))}
      </div>
    </ToastCtx.Provider>
  );
}

export function useToast(): ToastApi {
  return useContext(ToastCtx);
}

/* ---------------------------------------------------------------- segmented control */

export function Segmented<T extends string>({ label, value, options, onChange }: { label: string; value: T; options: { value: T; label: string }[]; onChange: (v: T) => void }) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((o) => (
        <button key={o.value} type="button" aria-pressed={value === o.value} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Polite screen-reader announcements for events like "Timer started". */
export function useAnnouncer(): [string, (msg: string) => void] {
  const [msg, setMsg] = useState("");
  const announce = useCallback((m: string) => {
    setMsg("");
    window.setTimeout(() => setMsg(m), 50);
  }, []);
  return [msg, announce];
}
