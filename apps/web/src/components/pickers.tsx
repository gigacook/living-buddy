import type { Category, Group, Member } from "@tendly/contracts";
import { CATEGORIES } from "../lib/categories";
import { WEEKDAY_CODES, WEEKDAY_NAMES, type RepeatPreset } from "../lib/recurrence";

/** Category picker. Deliberately neutral: the selected state uses outline and
 * weight, not the category's color, so the control itself stays calm. */
export function CategoryPicker({ value, onChange, label = "Category", allowAll }: { value: Category | null; onChange: (c: Category | null) => void; label?: string; allowAll?: boolean }) {
  return (
    <fieldset>
      <legend>{label}</legend>
      <div className="chips" role="radiogroup" aria-label={label} style={{ marginTop: 6 }}>
        {allowAll && (
          <button type="button" role="radio" aria-checked={value === null} className="chip" onClick={() => onChange(null)}>
            All
          </button>
        )}
        {CATEGORIES.map((c) => {
          const Icon = c.icon;
          return (
            <button key={c.key} type="button" role="radio" aria-checked={value === c.key} className="chip" onClick={() => onChange(c.key)} title={c.hint}>
              <Icon size={16} aria-hidden />
              {c.label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

export function MemberSelect({
  id,
  members,
  value,
  onChange,
  allowNone = true,
  noneLabel = "No one yet",
  describedBy,
}: {
  id: string;
  members: Member[];
  value: string | null;
  onChange: (v: string | null) => void;
  allowNone?: boolean;
  noneLabel?: string;
  describedBy?: string;
}) {
  return (
    <select id={id} className="select" value={value ?? ""} onChange={(e) => onChange(e.target.value || null)} aria-describedby={describedBy}>
      {allowNone && <option value="">{noneLabel}</option>}
      {members.map((m) => (
        <option key={m.id} value={m.id}>
          {m.displayName}
        </option>
      ))}
    </select>
  );
}

export function GroupSelect({ id, groups, value, onChange, noneLabel = "Just me (personal)" }: { id: string; groups: Group[]; value: string | null; onChange: (v: string | null) => void; noneLabel?: string }) {
  return (
    <select id={id} className="select" value={value ?? ""} onChange={(e) => onChange(e.target.value || null)}>
      <option value="">{noneLabel}</option>
      {groups.map((g) => (
        <option key={g.id} value={g.id}>
          {g.name}
        </option>
      ))}
    </select>
  );
}

export function RepeatPicker({
  preset,
  days,
  custom,
  onChange,
}: {
  preset: RepeatPreset;
  days: string[];
  custom: string;
  onChange: (p: { preset: RepeatPreset; days: string[]; custom: string }) => void;
}) {
  return (
    <div className="stack-sm">
      <div className="field">
        <label htmlFor="repeat-preset">Repeat</label>
        <select id="repeat-preset" className="select" value={preset} onChange={(e) => onChange({ preset: e.target.value as RepeatPreset, days, custom })}>
          <option value="none">Doesn't repeat</option>
          <option value="daily">Every day</option>
          <option value="weekdays">Every weekday</option>
          <option value="weekly">Every week</option>
          <option value="biweekly">Every 2 weeks</option>
          <option value="monthly">Every month</option>
          <option value="custom">Custom rule…</option>
        </select>
      </div>
      {(preset === "weekly" || preset === "biweekly") && (
        <fieldset>
          <legend className="small">On these days (optional)</legend>
          <div className="chips" style={{ marginTop: 6 }}>
            {WEEKDAY_CODES.map((code, i) => (
              <button
                key={code}
                type="button"
                className="chip"
                aria-pressed={days.includes(code)}
                onClick={() => onChange({ preset, custom, days: days.includes(code) ? days.filter((d) => d !== code) : [...days, code].sort((a, b) => WEEKDAY_CODES.indexOf(a as never) - WEEKDAY_CODES.indexOf(b as never)) })}
              >
                {WEEKDAY_NAMES[i]}
              </button>
            ))}
          </div>
        </fieldset>
      )}
      {preset === "custom" && (
        <div className="field">
          <label htmlFor="repeat-custom">Repeat rule (iCalendar RRULE)</label>
          <input id="repeat-custom" className="input" value={custom} placeholder="FREQ=MONTHLY;BYDAY=-1FR" onChange={(e) => onChange({ preset, days, custom: e.target.value })} />
          <span className="hint">Supports DAILY/WEEKLY/MONTHLY/YEARLY with INTERVAL, COUNT, UNTIL, BYDAY, BYMONTHDAY, BYMONTH.</span>
        </div>
      )}
    </div>
  );
}
