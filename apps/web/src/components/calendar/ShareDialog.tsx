import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Copy, Link2Off } from "lucide-react";
import type { CreatedShareLink, ShareDetail } from "@tendly/contracts";
import { api, errorMessage } from "../../lib/api";
import { keys, useGroups, useMe, useSession } from "../../lib/queries";
import { Dialog, Field, useToast } from "../ui";
import { useSourcesQuery } from "./hooks";

function absolute(path: string): string {
  return `${window.location.origin}${path}`;
}

/** Read-only share links. Each link is a secret: anyone who has it can view what it covers. */
export function ShareDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const session = useSession();
  const qc = useQueryClient();
  const toast = useToast();
  const me = useMe();
  const { data: links = [] } = useQuery({ queryKey: keys.shares, queryFn: api.shares, enabled: open });
  const { data: groups = [] } = useGroups();
  const { data: sources = [] } = useSourcesQuery(open);
  const myGroups = groups.filter((g) => me && g.memberIds.includes(me.id));
  const [label, setLabel] = useState("");
  const [groupIds, setGroupIds] = useState<string[]>([]);
  const [sourceIds, setSourceIds] = useState<string[]>([]);
  const [includeTasks, setIncludeTasks] = useState(true);
  const [includePersonal, setIncludePersonal] = useState(false);
  const [detail, setDetail] = useState<ShareDetail>("titles_only");
  const [days, setDays] = useState(60);
  const [expires, setExpires] = useState<number | "">(90);
  const [created, setCreated] = useState<CreatedShareLink | null>(null);
  const [error, setError] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () =>
      api.createShare({
        label,
        expiresInDays: expires === "" ? undefined : expires,
        scope: {
          groupIds,
          sourceIds,
          memberIds: includePersonal && me ? [me.id] : [],
          categories: [],
          includeTasks,
          includePersonal,
          detail,
          daysBack: 7,
          daysAhead: days,
        },
      }),
    onSuccess: (c) => {
      setCreated(c);
      qc.invalidateQueries({ queryKey: keys.shares });
    },
    onError: (e) => setError(errorMessage(e)),
  });
  const revoke = useMutation({
    mutationFn: (id: string) => api.revokeShare(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: keys.shares });
      toast.show("Link revoked. It stops working immediately.");
    },
  });
  const copy = (text: string) => navigator.clipboard?.writeText(text).then(() => toast.show("Copied"), () => toast.show("Copy failed — select the text instead."));
  const toggle = (list: string[], set: (v: string[]) => void, id: string) => set(list.includes(id) ? list.filter((x) => x !== id) : [...list, id]);

  return (
    <Dialog open={open} onClose={() => { setCreated(null); onClose(); }} title="Share a read-only calendar" wide>
      {!session.data?.sharingEnabled ? (
        <div className="banner banner-warn" role="status">
          Sharing is turned off. Whoever runs Tendly can turn it on under Settings → Administration on the host computer.
        </div>
      ) : created ? (
        <div className="stack">
          <div className="banner banner-ok" role="status">
            Link created. Copy it now — for safety Tendly only stores a fingerprint and can't show it again.
          </div>
          {[
            ["Web page", created.htmlPath],
            ["Calendar subscription (.ics)", created.icsPath],
            ["JSON", created.jsonPath],
          ].map(([name, path]) => (
            <div key={path} className="stack-sm">
              <strong>{name}</strong>
              <div className="row">
                <code className="code-box" style={{ flex: 1 }}>
                  {absolute(path)}
                </code>
                <button type="button" className="btn" onClick={() => copy(absolute(path))}>
                  <Copy size={16} aria-hidden /> Copy
                </button>
              </div>
            </div>
          ))}
          <p className="small muted">Calendar apps poll subscriptions on their own schedule (often every few hours), so changes aren't instant. Anyone with the link can view it; revoke it any time.</p>
          <button type="button" className="btn" onClick={() => setCreated(null)}>
            Done
          </button>
        </div>
      ) : (
        <div className="stack">
          <form
            className="stack"
            onSubmit={(e) => {
              e.preventDefault();
              setError(null);
              create.mutate();
            }}
          >
            {error && (
              <div className="banner banner-danger" role="alert">
                {error}
              </div>
            )}
            <Field label="Name for this link">{({ id }) => <input id={id} className="input" value={label} onChange={(e) => setLabel(e.target.value)} required placeholder="For grandma" />}</Field>
            <fieldset>
              <legend>Include groups</legend>
              <div className="chips" style={{ marginTop: 6 }}>
                {myGroups.map((g) => (
                  <button key={g.id} type="button" className="chip" aria-pressed={groupIds.includes(g.id)} onClick={() => toggle(groupIds, setGroupIds, g.id)}>
                    {g.name}
                  </button>
                ))}
                {myGroups.length === 0 && <span className="small muted">You're not in any groups yet.</span>}
              </div>
            </fieldset>
            <fieldset>
              <legend>Include calendars</legend>
              <p className="small muted" style={{ margin: "2px 0 6px" }}>Private calendars are only shared if you pick them here.</p>
              <div className="chips">
                {sources.map((s) => (
                  <button key={s.id} type="button" className="chip" aria-pressed={sourceIds.includes(s.id)} onClick={() => toggle(sourceIds, setSourceIds, s.id)}>
                    {s.name}
                    {s.isPrivate ? " (private)" : ""}
                  </button>
                ))}
              </div>
            </fieldset>
            <label className="check">
              <input type="checkbox" checked={includeTasks} onChange={(e) => setIncludeTasks(e.target.checked)} />
              Include dated tasks from the chosen groups
            </label>
            <label className="check">
              <input type="checkbox" checked={includePersonal} onChange={(e) => setIncludePersonal(e.target.checked)} />
              Also include my personal tasks
            </label>
            <div className="form-grid">
              <Field label="How much detail">
                {({ id }) => (
                  <select id={id} className="select" value={detail} onChange={(e) => setDetail(e.target.value as ShareDetail)}>
                    <option value="busy_only">Busy times only</option>
                    <option value="titles_only">Titles and times</option>
                    <option value="full">Everything (notes and places)</option>
                  </select>
                )}
              </Field>
              <Field label="Days ahead">{({ id }) => <input id={id} type="number" min={1} max={400} className="input" value={days} onChange={(e) => setDays(Number(e.target.value) || 1)} />}</Field>
              <Field label="Expires after (days)" hint="Leave empty to keep until revoked.">
                {({ id, describedBy }) => <input id={id} type="number" min={1} max={366} className="input" value={expires} onChange={(e) => setExpires(e.target.value === "" ? "" : Number(e.target.value))} aria-describedby={describedBy} />}
              </Field>
            </div>
            <button type="submit" className="btn btn-primary" disabled={create.isPending || !label.trim()}>
              Create link
            </button>
          </form>
          <section aria-labelledby="links-heading">
            <h3 id="links-heading">Existing links</h3>
            {links.length === 0 ? (
              <p className="muted small">No links yet.</p>
            ) : (
              <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
                {links.map((l) => (
                  <li key={l.id} className="row-between card-quiet">
                    <span>
                      <strong>{l.label}</strong>
                      <br />
                      <span className="small muted">
                        {l.revokedAt ? "Revoked" : l.expiresAt && new Date(l.expiresAt) < new Date() ? "Expired" : "Active"} · viewed {l.useCount} times · {l.scope.detail.replace("_", " ")}
                      </span>
                    </span>
                    {!l.revokedAt && (
                      <button type="button" className="btn btn-sm btn-danger" onClick={() => revoke.mutate(l.id)}>
                        <Link2Off size={16} aria-hidden /> Revoke
                      </button>
                    )}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </div>
      )}
    </Dialog>
  );
}
