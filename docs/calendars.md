# Calendars

## Sources

| Kind | How it gets data | Editable in Tendly? |
| --- | --- | --- |
| **Tendly calendar** (local) | Events you create | Yes |
| **Imported file** | An `.ics` file you upload; re-importing the same file into the same calendar updates it | Local overrides only |
| **Subscription** | An `https://` or `webcal://` link Tendly polls about every hour (configurable 15 min – 24 h) | Local overrides only |

Each source can be private (only you), shared (everyone on this Tendly) or attached to a group (that group's members).

**Polling is not push.** A subscription only changes when Tendly next downloads it, and calendar apps that subscribe to Tendly's own feeds refresh on *their* schedule (often several hours). Tendly does **not** write changes back to Google, Apple, Outlook or any other provider. You can add a private note, change the category, or hide an event locally; those overrides survive refreshes.

Provider write-back (CalDAV and provider APIs) is not implemented. The boundary for it is the calendar source kind; a future CalDAV adapter would be a new source kind with its own authorization and conflict handling.

## Standards support

- RFC 5545 parsing with line unfolding, parameter quoting, TEXT escaping and safe limits (5 MB, 20 000 events, nesting depth 8).
- `DTSTART`/`DTEND`/`DURATION`, all-day (`VALUE=DATE`, exclusive end), UTC, floating and `TZID` times. IANA zone names, common Windows zone names and vendor-prefixed names are recognized; unknown zones fall back to the calendar's default zone with a warning. VTIMEZONE rule bodies from feeds are not evaluated; Tendly uses the IANA database instead.
- `RRULE` with `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`, `INTERVAL`, `COUNT`, `UNTIL`, `BYDAY` (including ordinals such as `-1FR`), `BYMONTHDAY` (including negative days), `BYMONTH`, `WKST`; `EXDATE`, `RDATE`; `RECURRENCE-ID` overrides. Expansion happens in wall-clock time, so "every Monday 09:00" stays at 09:00 across DST changes. Times that fall in a DST gap use the pre-gap offset and ambiguous times use the first occurrence (RFC 5545 rules).
- Unsupported rule parts (`BYSETPOS`, `BYHOUR`, `BYWEEKNO`, `HOURLY`, …) are reported as warnings and only the first occurrence is shown, rather than guessing.
- Events without a `UID` get a stable generated UID from their title and start, so refreshes don't create duplicates.
- `STATUS:CANCELLED` and `METHOD:CANCEL` are honored.

## Refresh and merge rules

Events are keyed by *(source, UID, RECURRENCE-ID)*:

1. A new key is inserted.
2. A higher `SEQUENCE` always replaces the stored version; a lower one is ignored.
3. On equal `SEQUENCE`, identical content is a no-op; a newer `LAST-MODIFIED` wins; otherwise the feed is treated as authoritative.
4. When a refreshed feed no longer contains an event, it is marked "removed upstream" — **for that source only**. Other sources and local data are never touched. Deleting a source removes only its own events.
5. In merged views, the same UID from several sources is shown once. The winner is chosen by `SEQUENCE`, then `LAST-MODIFIED`, then source priority, then source ID, so the result never depends on fetch order. The others are listed as "also in".

## Export and feeds

- **Download:** Calendar → Export → `.ics` or JSON, using the current filters (30 days back, one year ahead).
- **ICS output:** stable UIDs, `DTSTAMP`, `LAST-MODIFIED`, `SEQUENCE`, generated `VTIMEZONE` blocks, escaped and 75-octet-folded lines. Recurring events are exported as masters with `RRULE`/`EXDATE` plus overrides. Tasks with due dates are exported as events with `UID:task-<id>@tendly`.
- **API:** `GET /api/calendar/occurrences`, `/api/calendar/export.ics`, `/api/calendar/export.json` accept `from`, `to`, `groupId`, `memberId`, `category`, `sourceId`, `includeTasks`.

## Sharing

Create read-only links under Calendar → Share (after an administrator enables sharing). Each link has an HTML page (no JavaScript), an `.ics` subscription feed and a JSON feed. See [security.md](security.md#share-links) for how links are protected.

## Change history

Every create, update, cancellation, removal upstream, import, refresh ("poll"), local override, publish and revoke is recorded with: who (display name, or **"external source / unknown actor"** for feed changes, since feeds don't say who edited them), server timestamp, source, operation, object revision, and before/after details. View it under Calendar → Calendars → Changes or via `GET /api/calendar/changes`.
