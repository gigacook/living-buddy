import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight, Download, Filter, Layers, Plus, Share2 } from "lucide-react";
import type { Category, EventOccurrence } from "@tendly/contracts";
import { api, errorMessage, type RangeFilters } from "../lib/api";
import { addDays, isoDate, localTimezone, longDate, monthTitle, parseIsoDate, timeOf, todayIso, weekdayShort } from "../lib/dates";
import { keys, useGroups, useMembers } from "../lib/queries";
import { setPrefs, usePrefs } from "../lib/prefs";
import { CategoryPicker, GroupSelect, MemberSelect } from "../components/pickers";
import { CategoryBadge, EmptyState, ErrorState, Field, Segmented, Tip, useToast } from "../components/ui";
import { SourcesDialog } from "../components/calendar/SourcesDialog";
import { ShareDialog } from "../components/calendar/ShareDialog";
import { EventDialog, NewEventDialog } from "../components/calendar/EventDialogs";

type View = "month" | "week" | "agenda";

function startOfWeek(iso: string): string {
  const d = parseIsoDate(iso);
  const dow = (d.getDay() + 6) % 7; // Monday = 0
  return addDays(iso, -dow);
}

function occDay(o: EventOccurrence): string {
  return o.allDay && o.startDate ? o.startDate : isoDate(new Date(o.start));
}

function occDays(o: EventOccurrence): string[] {
  if (o.allDay && o.startDate && o.endDate) {
    const out: string[] = [];
    let d = o.startDate;
    while (d <= o.endDate && out.length < 60) {
      out.push(d);
      d = addDays(d, 1);
    }
    return out;
  }
  return [occDay(o)];
}

function itemLabel(o: EventOccurrence): string {
  return `${o.allDay ? "" : timeOf(new Date(o.start)) + " "}${o.kind === "task" ? "☐ " : ""}${o.title}`;
}

export default function Calendar() {
  const prefs = usePrefs();
  const view = prefs.calendarView;
  const [anchor, setAnchor] = useState(todayIso());
  const [category, setCategory] = useState<Category | null>(null);
  const [groupId, setGroupId] = useState<string | null>(null);
  const [memberId, setMemberId] = useState<string | null>(null);
  const [includeTasks, setIncludeTasks] = useState(true);
  const [showFilters, setShowFilters] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [shareOpen, setShareOpen] = useState(false);
  const [newEvent, setNewEvent] = useState<string | null>(null);
  const [selected, setSelected] = useState<EventOccurrence | null>(null);
  const { data: groups = [] } = useGroups();
  const { data: members = [] } = useMembers();
  const toast = useToast();

  const range = useMemo(() => {
    if (view === "month") {
      const d = parseIsoDate(anchor);
      const first = isoDate(new Date(d.getFullYear(), d.getMonth(), 1));
      const start = startOfWeek(first);
      return { start, days: 42 };
    }
    if (view === "week") return { start: startOfWeek(anchor), days: 7 };
    return { start: anchor, days: 30 };
  }, [view, anchor]);

  const filters: RangeFilters = {
    from: range.start,
    to: addDays(range.start, range.days),
    category: category ?? undefined,
    groupId: groupId ?? undefined,
    memberId: memberId ?? undefined,
    includeTasks,
  };
  const q = useQuery({ queryKey: keys.occurrences(filters), queryFn: () => api.occurrences(filters) });
  const byDay = useMemo(() => {
    const m = new Map<string, EventOccurrence[]>();
    for (const o of q.data ?? []) for (const d of occDays(o)) m.set(d, [...(m.get(d) ?? []), o]);
    return m;
  }, [q.data]);

  const move = (dir: number) => {
    const d = parseIsoDate(anchor);
    if (view === "month") setAnchor(isoDate(new Date(d.getFullYear(), d.getMonth() + dir, 1)));
    else setAnchor(addDays(anchor, dir * (view === "week" ? 7 : 30)));
  };
  const title =
    view === "month"
      ? monthTitle(parseIsoDate(anchor).getFullYear(), parseIsoDate(anchor).getMonth())
      : view === "week"
        ? `Week of ${longDate(range.start)}`
        : `From ${longDate(anchor)}`;
  const today = todayIso();

  const download = async (kind: "ics" | "json") => {
    try {
      const body = await api.exportText(kind, { ...filters, from: addDays(today, -30), to: addDays(today, 365) });
      const text = typeof body === "string" ? body : JSON.stringify(body, null, 2);
      const blob = new Blob([text], { type: kind === "ics" ? "text/calendar" : "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = kind === "ics" ? "tendly.ics" : "tendly-calendar.json";
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(url);
    } catch (e) {
      toast.show(errorMessage(e));
    }
  };

  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Calendar</h1>
          <p>Your events, shared calendars and dated tasks together.</p>
        </div>
        <div className="row">
          <button type="button" className="btn" onClick={() => setSourcesOpen(true)}>
            <Layers size={18} aria-hidden /> Calendars
          </button>
          <button type="button" className="btn" onClick={() => setShareOpen(true)}>
            <Share2 size={18} aria-hidden /> Share
          </button>
          <button type="button" className="btn btn-primary" onClick={() => setNewEvent(today)}>
            <Plus size={18} aria-hidden /> Event
          </button>
        </div>
      </div>

      <div className="row-between">
        <div className="row">
          <button type="button" className="btn btn-icon" onClick={() => move(-1)} aria-label="Previous">
            <ChevronLeft size={20} aria-hidden />
          </button>
          <button type="button" className="btn" onClick={() => setAnchor(today)}>
            Today
          </button>
          <button type="button" className="btn btn-icon" onClick={() => move(1)} aria-label="Next">
            <ChevronRight size={20} aria-hidden />
          </button>
          <h2 style={{ margin: "0 8px" }} aria-live="polite">
            {title}
          </h2>
        </div>
        <div className="row">
          <Segmented
            label="Calendar view"
            value={view}
            onChange={(v: View) => setPrefs({ calendarView: v })}
            options={[
              { value: "month", label: "Month" },
              { value: "week", label: "Week" },
              { value: "agenda", label: "Agenda" },
            ]}
          />
          <button type="button" className="btn" aria-expanded={showFilters} onClick={() => setShowFilters(!showFilters)}>
            <Filter size={18} aria-hidden /> Filter
          </button>
          <details className="menu" style={{ position: "relative" }}>
            <summary className="btn">
              <Download size={18} aria-hidden /> Export
            </summary>
            <div className="card stack-sm" style={{ position: "absolute", right: 0, zIndex: 10, minWidth: 220 }}>
              <button type="button" className="btn" onClick={() => download("ics")}>
                Download .ics
              </button>
              <button type="button" className="btn" onClick={() => download("json")}>
                Download JSON
              </button>
              <span className="small muted">Uses the current filters, 30 days back to a year ahead.</span>
            </div>
          </details>
        </div>
      </div>

      {showFilters && (
        <div className="card-quiet stack">
          <CategoryPicker value={category} onChange={setCategory} allowAll />
          <div className="form-grid">
            <Field label="Group">{({ id }) => <GroupSelect id={id} groups={groups} value={groupId} onChange={setGroupId} noneLabel="All" />}</Field>
            <Field label="Person">{({ id }) => <MemberSelect id={id} members={members} value={memberId} onChange={setMemberId} noneLabel="Everyone" />}</Field>
          </div>
          <label className="check">
            <input type="checkbox" checked={includeTasks} onChange={(e) => setIncludeTasks(e.target.checked)} />
            Show tasks with due dates
          </label>
        </div>
      )}

      <Tip id="calendar-intro" pose="happy">
        <p className="small">Add calendars under “Calendars”: import an .ics file or paste a subscription link. Subscriptions refresh about hourly — they're read-only copies, so edits happen in the original app.</p>
      </Tip>

      {q.error && <ErrorState message={errorMessage(q.error)} onRetry={() => q.refetch()} />}

      {view === "month" && (
        <div className="cal-grid">
          {Array.from({ length: 7 }).map((_, i) => (
            <div key={i} className="cal-dow" aria-hidden>
              {weekdayShort(i)}
            </div>
          ))}
          {Array.from({ length: 6 }).map((_, w) => (
            <div key={w} style={{ display: "contents" }}>
              {Array.from({ length: 7 }).map((__, i) => {
                const day = addDays(range.start, w * 7 + i);
                const items = byDay.get(day) ?? [];
                const outside = parseIsoDate(day).getMonth() !== parseIsoDate(anchor).getMonth();
                return (
                  <div key={day} role="group" className={`cal-cell${outside ? " outside" : ""}${day === today ? " today" : ""}`} aria-label={`${longDate(day)}, ${items.length} item${items.length === 1 ? "" : "s"}`}>
                    <button type="button" className="cal-day" onClick={() => setNewEvent(day)} aria-label={`Add event on ${longDate(day)}`}>
                      {parseIsoDate(day).getDate()}
                    </button>
                    {items.slice(0, 3).map((o) => (
                      <button key={`${o.eventId}-${o.instanceKey}`} type="button" className={`cal-item cat cat-${o.category ?? "none"}`} onClick={() => setSelected(o)}>
                        {itemLabel(o)}
                      </button>
                    ))}
                    {items.length > 3 && <span className="cal-more">+{items.length - 3} more</span>}
                  </div>
                );
              })}
            </div>
          ))}
        </div>
      )}

      {view === "week" && (
        <div className="week-grid">
          {Array.from({ length: 7 }).map((_, i) => {
            const day = addDays(range.start, i);
            const items = byDay.get(day) ?? [];
            return (
              <section key={day} className={`week-col${day === today ? " today" : ""}`} aria-label={longDate(day)}>
                <strong>
                  {weekdayShort(i)} {parseIsoDate(day).getDate()}
                </strong>
                {items.map((o) => (
                  <button key={`${o.eventId}-${o.instanceKey}`} type="button" className={`cal-item cat cat-${o.category ?? "none"}`} style={{ whiteSpace: "normal" }} onClick={() => setSelected(o)}>
                    {itemLabel(o)}
                  </button>
                ))}
                <button type="button" className="btn btn-ghost btn-sm" onClick={() => setNewEvent(day)} aria-label={`Add event on ${longDate(day)}`}>
                  <Plus size={16} aria-hidden />
                </button>
              </section>
            );
          })}
        </div>
      )}

      {view === "agenda" &&
        ((q.data ?? []).length === 0 && !q.isLoading ? (
          <EmptyState title="Nothing in the next 30 days" pose="sleepy" />
        ) : (
          [...byDay.entries()]
            .sort(([a], [b]) => a.localeCompare(b))
            .map(([day, items]) => (
              <section key={day} className="agenda-day" aria-label={longDate(day)}>
                <h3>{longDate(day)}</h3>
                {items.map((o) => (
                  <button key={`${o.eventId}-${o.instanceKey}-${day}`} type="button" className={`agenda-item cat cat-${o.category ?? "none"}`} onClick={() => setSelected(o)}>
                    <span className="agenda-time">{o.allDay ? "All day" : `${timeOf(new Date(o.start))}–${timeOf(new Date(o.end))}`}</span>
                    <span className="stack-sm" style={{ gap: 4 }}>
                      <strong>{o.kind === "task" ? `Task: ${o.title}` : o.title}</strong>
                      <span className="row small">
                        <CategoryBadge category={o.category} />
                        {o.sourceName && <span className="muted">{o.sourceName}</span>}
                        {o.alsoIn.length > 0 && <span className="muted">also in {o.alsoIn.length} other calendar{o.alsoIn.length > 1 ? "s" : ""}</span>}
                      </span>
                    </span>
                  </button>
                ))}
              </section>
            ))
        ))}

      <SourcesDialog open={sourcesOpen} onClose={() => setSourcesOpen(false)} />
      <ShareDialog open={shareOpen} onClose={() => setShareOpen(false)} />
      <NewEventDialog date={newEvent} onClose={() => setNewEvent(null)} timezone={localTimezone()} />
      <EventDialog occurrence={selected} onClose={() => setSelected(null)} />
    </div>
  );
}

