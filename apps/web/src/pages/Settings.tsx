import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { KeyRound, Plug, Shield, UserRound } from "lucide-react";
import type { Connector, MemberPrefs } from "@tendly/contracts";
import { api, errorMessage, setAdminToken } from "../lib/api";
import { keys, useMe, useSession } from "../lib/queries";
import { setActorId, setPrefs, usePrefs } from "../lib/prefs";
import { Field, Segmented, useToast } from "../components/ui";
import { isNative } from "../lib/transport";

export default function Settings() {
  const session = useSession();
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Settings</h1>
          <p>Make Tendly calmer, quieter or more helpful.</p>
        </div>
      </div>
      <Profile />
      <Appearance />
      <DataSection />
      {session.data?.adminAvailable ? <Admin /> : <AdminLocked remote={session.data?.mode === "remote"} />}
      <Privacy />
    </div>
  );
}

function Profile() {
  const me = useMe();
  const qc = useQueryClient();
  const toast = useToast();
  const [name, setName] = useState(me?.displayName ?? "");
  const [prefs, setP] = useState<MemberPrefs | null>(me?.prefs ?? null);
  useEffect(() => {
    if (me) {
      setName(me.displayName);
      setP(me.prefs);
    }
  }, [me]);
  const save = useMutation({
    mutationFn: () => api.updateMember(me!.id, { displayName: name, prefs: prefs ?? undefined }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: keys.members });
      toast.show("Saved");
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  if (!me || !prefs) return null;
  return (
    <section className="card stack" aria-labelledby="profile-h">
      <h2 id="profile-h" className="row">
        <UserRound size={20} aria-hidden /> You
      </h2>
      <form
        className="stack"
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate();
        }}
      >
        <div className="form-grid">
          <Field label="Display name" hint="A name tag for your groups, not a login.">
            {({ id, describedBy }) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} aria-describedby={describedBy} maxLength={40} />}
          </Field>
          <Field label="Time zone">{({ id }) => <input id={id} className="input" value={prefs.timezone} onChange={(e) => setP({ ...prefs, timezone: e.target.value })} />}</Field>
        </div>
        <label className="check">
          <input type="checkbox" checked={prefs.acceptsNudges} onChange={(e) => setP({ ...prefs, acceptsNudges: e.target.checked })} />
          Let people in my groups send me gentle reminders (max a few per day)
        </label>
        <div className="form-grid">
          <Field label="Quiet hours start" hint="Reminders from others wait until quiet hours end.">
            {({ id, describedBy }) => <input id={id} type="time" className="input" value={prefs.quietStart ?? ""} onChange={(e) => setP({ ...prefs, quietStart: e.target.value || null })} aria-describedby={describedBy} />}
          </Field>
          <Field label="Quiet hours end">{({ id }) => <input id={id} type="time" className="input" value={prefs.quietEnd ?? ""} onChange={(e) => setP({ ...prefs, quietEnd: e.target.value || null })} />}</Field>
        </div>
        <label className="check">
          <input type="checkbox" checked={prefs.showUsageCard} onChange={(e) => setP({ ...prefs, showUsageCard: e.target.checked })} />
          Show the Claude usage card (manual, optional)
        </label>
        <div className="row">
          <button type="submit" className="btn btn-primary" disabled={save.isPending}>
            Save
          </button>
          <button type="button" className="btn btn-ghost" onClick={() => setActorId(null)}>
            Switch person
          </button>
        </div>
      </form>
    </section>
  );
}

function Appearance() {
  const prefs = usePrefs();
  return (
    <section className="card stack" aria-labelledby="look-h">
      <h2 id="look-h">Look and feel (this device)</h2>
      <div className="stack-sm">
        <span className="label" id="theme-l" style={{ fontWeight: 600 }}>
          Theme
        </span>
        <Segmented
          label="Theme"
          value={prefs.theme}
          onChange={(theme) => setPrefs({ theme })}
          options={[
            { value: "system", label: "Match system" },
            { value: "light", label: "Light" },
            { value: "dark", label: "Dark" },
          ]}
        />
      </div>
      <label className="check">
        <input type="checkbox" checked={prefs.motion === "reduced"} onChange={(e) => setPrefs({ motion: e.target.checked ? "reduced" : "system" })} />
        Reduce motion (always on when your system asks for it)
      </label>
      <label className="check">
        <input type="checkbox" checked={prefs.quiet} onChange={(e) => setPrefs({ quiet: e.target.checked })} />
        Quiet mode — hide Pim and tips, mute sounds
      </label>
      <label className="check">
        <input type="checkbox" checked={prefs.sound} onChange={(e) => setPrefs({ sound: e.target.checked })} />
        Soft chime for timers and alarms
      </label>
      {prefs.dismissedTips.length > 0 && (
        <button type="button" className="btn btn-sm" style={{ alignSelf: "flex-start" }} onClick={() => setPrefs({ dismissedTips: [] })}>
          Show dismissed tips again
        </button>
      )}
    </section>
  );
}

function DataSection() {
  const session = useSession();
  const toast = useToast();
  const exportAll = async () => {
    try {
      const data = await api.admin.exportAll();
      const url = URL.createObjectURL(new Blob([JSON.stringify(data, null, 2)], { type: "application/json" }));
      const a = document.createElement("a");
      a.href = url;
      a.download = "tendly-export.json";
      a.click();
      URL.revokeObjectURL(url);
    } catch (e) {
      toast.show(errorMessage(e));
    }
  };
  return (
    <section className="card stack-sm" aria-labelledby="data-h">
      <h2 id="data-h">Your data</h2>
      <p className="small muted">
        Everything is stored in one SQLite file on the computer running Tendly. Export your calendar from the Calendar page. Full backups: run <span className="kbd">tendly backup --out backup.db</span> on the host.
      </p>
      {session.data?.adminAvailable && (
        <button type="button" className="btn" style={{ alignSelf: "flex-start" }} onClick={exportAll}>
          Export everything (JSON, no secrets)
        </button>
      )}
    </section>
  );
}

function AdminLocked({ remote }: { remote: boolean }) {
  const [token, setToken] = useState("");
  const qc = useQueryClient();
  return (
    <section className="card stack-sm" aria-labelledby="admin-h">
      <h2 id="admin-h" className="row">
        <Shield size={20} aria-hidden /> Administration
      </h2>
      {remote ? (
        <form
          className="stack-sm"
          onSubmit={(e) => {
            e.preventDefault();
            setAdminToken(token);
            qc.invalidateQueries({ queryKey: keys.session });
          }}
        >
          <p className="small muted">Enter the server's admin token. It's kept in memory for this tab only.</p>
          <Field label="Admin token">{({ id }) => <input id={id} type="password" className="input" value={token} onChange={(e) => setToken(e.target.value)} autoComplete="off" />}</Field>
          <button type="submit" className="btn">
            Unlock
          </button>
        </form>
      ) : (
        <p className="small muted">Mailbox connections, AI keys and sharing can only be changed on the computer that runs Tendly. That keeps them away from anyone who just knows a name on the network.</p>
      )}
    </section>
  );
}

function Admin() {
  const qc = useQueryClient();
  const toast = useToast();
  const settings = useQuery({ queryKey: ["admin", "settings"], queryFn: api.admin.settings });
  const sharing = useMutation({
    mutationFn: (v: boolean) => api.admin.patchSettings({ sharingEnabled: v }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["admin"] });
      qc.invalidateQueries({ queryKey: keys.session });
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  return (
    <section className="card stack" aria-labelledby="admin-h">
      <h2 id="admin-h" className="row">
        <Shield size={20} aria-hidden /> Administration
      </h2>
      {settings.data && (
        <p className="small muted">
          Mode: <strong>{settings.data.mode}</strong> · listening on {settings.data.bind}
          {settings.data.allowedNetworks.length ? ` · allowed networks ${settings.data.allowedNetworks.join(", ")}` : ""} · encryption key from {settings.data.encryptionKeySource}
        </p>
      )}
      <label className="check">
        <input type="checkbox" checked={settings.data?.sharingEnabled ?? false} onChange={(e) => sharing.mutate(e.target.checked)} />
        Allow read-only share links (turning this off stops all existing links)
      </label>
      <AiSection />
      <ConnectorsSection />
      {settings.data?.mode === "remote" && <DevicesSection />}
    </section>
  );
}

function AiSection() {
  const qc = useQueryClient();
  const toast = useToast();
  const ai = useQuery({ queryKey: ["admin", "ai"], queryFn: api.admin.ai });
  const [provider, setProvider] = useState("none");
  const [model, setModel] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [key, setKey] = useState("");
  useEffect(() => {
    if (ai.data) {
      setProvider(ai.data.provider);
      setModel(ai.data.model);
      setBaseUrl(ai.data.baseUrl ?? "");
    }
  }, [ai.data]);
  const save = useMutation({
    mutationFn: (extra: { clearKey?: boolean; allowPaste?: boolean } = {}) =>
      api.admin.putAi({
        provider,
        model: model || undefined,
        baseUrl: provider === "openai_compatible" && baseUrl ? baseUrl : undefined,
        apiKey: key || undefined,
        clearKey: extra.clearKey,
        allowPasteIntake: extra.allowPaste,
      }),
    onSuccess: (v) => {
      setKey("");
      qc.setQueryData(["admin", "ai"], v);
      qc.invalidateQueries({ queryKey: keys.session });
      toast.show("AI settings saved");
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const test = useMutation({ mutationFn: api.admin.testAi, onSuccess: (r) => toast.show(r.ok ? `Provider works (${r.suggestions} suggestion found in a sample).` : `Provider error: ${r.error}`) });
  return (
    <details className="disclosure">
      <summary>
        <KeyRound size={18} aria-hidden /> AI extraction (bring your own key)
      </summary>
      <form
        className="stack"
        style={{ marginTop: 8 }}
        onSubmit={(e) => {
          e.preventDefault();
          save.mutate({});
        }}
      >
        <p className="small muted">
          Optional. Without a provider, Tendly uses simple local rules. With one, short redacted excerpts of messages you allow are sent to that provider, which may cost you money on your own account. Keys stay on this server, encrypted, and are never shown again.
        </p>
        <div className="form-grid">
          <Field label="Provider">
            {({ id }) => (
              <select id={id} className="select" value={provider} onChange={(e) => setProvider(e.target.value)}>
                <option value="none">None (local rules only)</option>
                <option value="anthropic">Anthropic (Claude)</option>
                <option value="openai_compatible">OpenAI-compatible endpoint (e.g. a local model)</option>
              </select>
            )}
          </Field>
          <Field label="Model">{({ id }) => <input id={id} className="input" value={model} onChange={(e) => setModel(e.target.value)} placeholder={provider === "anthropic" ? "claude-opus-5-5" : "llama3.1"} />}</Field>
          {provider === "openai_compatible" && <Field label="Base URL">{({ id }) => <input id={id} className="input" value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="http://127.0.0.1:11434/v1" />}</Field>}
          <Field label={ai.data?.hasKey ? `API key (set via ${ai.data.keySource})` : "API key"} hint="Write-only.">
            {({ id, describedBy }) => <input id={id} type="password" className="input" value={key} onChange={(e) => setKey(e.target.value)} autoComplete="off" aria-describedby={describedBy} />}
          </Field>
        </div>
        <label className="check">
          <input type="checkbox" checked={ai.data?.allowPasteIntake ?? false} onChange={(e) => save.mutate({ allowPaste: e.target.checked })} />
          Let people use AI for text they paste into the Inbox
        </label>
        <div className="row">
          <button type="submit" className="btn btn-primary">
            Save
          </button>
          <button type="button" className="btn" onClick={() => test.mutate()} disabled={!ai.data?.hasKey && provider !== "openai_compatible"}>
            Test
          </button>
          {ai.data?.keySource === "stored" && (
            <button type="button" className="btn btn-ghost btn-danger" onClick={() => save.mutate({ clearKey: true })}>
              Remove stored key
            </button>
          )}
        </div>
      </form>
    </details>
  );
}

function ConnectorsSection() {
  const qc = useQueryClient();
  const toast = useToast();
  const providers = useQuery({ queryKey: ["admin", "providers"], queryFn: api.admin.providers });
  const list = useQuery({ queryKey: ["admin", "connectors"], queryFn: api.admin.connectors });
  const [provider, setProvider] = useState("fixture");
  const [name, setName] = useState("");
  const [channels, setChannels] = useState("");
  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["admin", "connectors"] });
    qc.invalidateQueries({ queryKey: ["suggestions"] });
    qc.invalidateQueries({ queryKey: keys.inboxCount });
  };
  const create = useMutation({
    mutationFn: () => api.admin.createConnector({ provider, displayName: name || providers.data?.find((p) => p.provider === provider)?.name || provider, settings: provider === "slack" ? { channels: channels.split(",").map((c) => c.trim()).filter(Boolean) } : provider === "fixture" ? { set: "sample" } : {} }),
    onSuccess: () => {
      setName("");
      refresh();
    },
    onError: (e) => toast.show(errorMessage(e)),
  });
  const act = useMutation({
    mutationFn: async ({ c, action, token }: { c: Connector; action: string; token?: string }) => {
      if (action === "toggle") return api.admin.patchConnector(c.id, { enabled: !c.enabled });
      if (action === "ai") return api.admin.patchConnector(c.id, { aiConsent: !c.aiConsent });
      if (action === "run") return api.admin.runConnector(c.id);
      if (action === "disconnect") return api.admin.disconnect(c.id);
      if (action === "delete") return api.admin.deleteConnector(c.id);
      if (action === "token") return api.admin.setToken(c.id, token!);
      if (action === "oauth") {
        const { url } = await api.admin.oauthStart(c.provider, c.id);
        if (isNative()) window.open(url, "_blank", "noopener");
        else window.location.assign(url);
      }
      return null;
    },
    onSuccess: refresh,
    onError: (e) => toast.show(errorMessage(e)),
  });
  const info = providers.data?.find((p) => p.provider === provider);
  return (
    <details className="disclosure">
      <summary>
        <Plug size={18} aria-hidden /> Mail and messaging connectors
      </summary>
      <div className="stack" style={{ marginTop: 8 }}>
        <p className="small muted">
          Connectors read new messages (subject and a short preview) and turn them into suggestions in the owner's Inbox. They never send, delete or change anything. Stored previews are deleted after the retention period.
        </p>
        {(list.data ?? []).map((c) => (
          <div key={c.id} className="card-quiet stack-sm">
            <div className="row-between">
              <strong>{c.displayName}</strong>
              <span className="row">
                <span className="badge badge-plain">{c.implementation}</span>
                <span className={c.status === "healthy" ? "badge badge-ok" : c.status === "needs_auth" || c.status === "error" ? "badge badge-danger" : "badge badge-plain"}>{c.status.replace("_", " ")}</span>
              </span>
            </div>
            <span className="small muted">
              {c.itemsIngested} messages read · keeps previews {c.retentionDays} days
              {c.lastRunAt ? ` · last run ${new Date(c.lastRunAt).toLocaleString()}` : ""}
            </span>
            {c.lastError && <span className="small" style={{ color: "var(--danger)" }}>{c.lastError}</span>}
            <div className="row">
              <label className="check">
                <input type="checkbox" checked={c.enabled} onChange={() => act.mutate({ c, action: "toggle" })} /> On
              </label>
              <label className="check">
                <input type="checkbox" checked={c.aiConsent} onChange={() => act.mutate({ c, action: "ai" })} /> Allow AI extraction
              </label>
              {(c.provider === "gmail" || c.provider === "microsoft_graph") && (
                <button type="button" className="btn btn-sm" onClick={() => act.mutate({ c, action: "oauth" })}>
                  {c.hasCredentials ? "Reconnect" : "Connect"}
                </button>
              )}
              {c.provider === "slack" && (
                <button
                  type="button"
                  className="btn btn-sm"
                  onClick={() => {
                    const t = window.prompt("Paste the Slack bot token (xoxb-…). It is stored encrypted and never shown again.");
                    if (t) act.mutate({ c, action: "token", token: t });
                  }}
                >
                  Set bot token
                </button>
              )}
              <button type="button" className="btn btn-sm" onClick={() => act.mutate({ c, action: "run" })} disabled={!c.enabled}>
                Check now
              </button>
              {c.hasCredentials && (
                <button type="button" className="btn btn-sm btn-ghost" onClick={() => act.mutate({ c, action: "disconnect" })}>
                  Disconnect
                </button>
              )}
              <button type="button" className="btn btn-sm btn-ghost btn-danger" onClick={() => window.confirm(`Delete “${c.displayName}” and its stored previews?`) && act.mutate({ c, action: "delete" })}>
                Delete
              </button>
            </div>
          </div>
        ))}
        <form
          className="stack-sm"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate();
          }}
        >
          <div className="form-grid">
            <Field label="Add a connector">
              {({ id }) => (
                <select id={id} className="select" value={provider} onChange={(e) => setProvider(e.target.value)}>
                  {(providers.data ?? []).map((p) => (
                    <option key={p.provider} value={p.provider}>
                      {p.name} ({p.implementation})
                    </option>
                  ))}
                </select>
              )}
            </Field>
            <Field label="Name">{({ id }) => <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="My mail" />}</Field>
            {provider === "slack" && <Field label="Channel IDs (comma separated)">{({ id }) => <input id={id} className="input" value={channels} onChange={(e) => setChannels(e.target.value)} placeholder="C0123ABC" />}</Field>}
          </div>
          {info && (
            <p className="small muted">
              {info.description}
              {info.requires.length ? ` Requires: ${info.requires.join(", ")}.` : ""}
            </p>
          )}
          <button type="submit" className="btn" style={{ alignSelf: "flex-start" }} disabled={info?.implementation === "scaffolded"}>
            Add connector
          </button>
        </form>
      </div>
    </details>
  );
}

function DevicesSection() {
  const qc = useQueryClient();
  const devices = useQuery({ queryKey: ["admin", "devices"], queryFn: api.admin.devices });
  const [name, setName] = useState("");
  const [code, setCode] = useState<string | null>(null);
  const create = useMutation({
    mutationFn: () => api.admin.createDevice(name),
    onSuccess: (d) => {
      setCode(d.code);
      setName("");
      qc.invalidateQueries({ queryKey: ["admin", "devices"] });
    },
  });
  const revoke = useMutation({ mutationFn: (id: string) => api.admin.revokeDevice(id), onSuccess: () => qc.invalidateQueries({ queryKey: ["admin", "devices"] }) });
  return (
    <details className="disclosure">
      <summary>Paired devices</summary>
      <div className="stack-sm" style={{ marginTop: 8 }}>
        {code && (
          <div className="banner banner-ok" role="status">
            Pairing code (shown once): <code className="code-box">{code}</code>
          </div>
        )}
        <ul className="stack-sm" style={{ listStyle: "none", padding: 0 }}>
          {(devices.data ?? []).map((d) => (
            <li key={d.id} className="row-between">
              <span>
                {d.name} <span className="small muted">{d.revokedAt ? "revoked" : d.lastSeenAt ? `seen ${new Date(d.lastSeenAt).toLocaleString()}` : "never used"}</span>
              </span>
              {!d.revokedAt && (
                <button type="button" className="btn btn-sm btn-danger" onClick={() => revoke.mutate(d.id)}>
                  Revoke
                </button>
              )}
            </li>
          ))}
        </ul>
        <form
          className="row"
          onSubmit={(e) => {
            e.preventDefault();
            create.mutate();
          }}
        >
          <label htmlFor="dev-name" className="visually-hidden">
            Device name
          </label>
          <input id="dev-name" className="input" style={{ flex: 1 }} value={name} onChange={(e) => setName(e.target.value)} placeholder="Sam's phone" />
          <button type="submit" className="btn" disabled={!name.trim()}>
            Create pairing code
          </button>
        </form>
      </div>
    </details>
  );
}

function Privacy() {
  return (
    <section className="card stack-sm" aria-labelledby="privacy-h">
      <h2 id="privacy-h">Privacy, plainly</h2>
      <ul className="small" style={{ paddingLeft: 18, margin: 0 }}>
        <li>Your tasks, calendars and history live in the database on the computer running Tendly. Tendly has no analytics or tracking.</li>
        <li>Nothing leaves that computer unless you: subscribe to a calendar link (Tendly downloads it), create a share link (anyone with it can read what it covers), connect a mailbox (Tendly reads previews from that provider), or turn on an AI provider (short redacted excerpts are sent to it).</li>
        <li>Names are labels, not logins. On a shared network, anyone who can open Tendly can choose any name.</li>
        <li>The Claude usage card only stores numbers you type in. Tendly never signs in to your Claude account.</li>
      </ul>
    </section>
  );
}
