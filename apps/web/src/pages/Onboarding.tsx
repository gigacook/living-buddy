import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiError, errorMessage } from "../lib/api";
import { localTimezone } from "../lib/dates";
import { setActorId } from "../lib/prefs";
import { keys, useMembers } from "../lib/queries";
import { Mascot } from "../components/Mascot";
import { Field } from "../components/ui";

/** Name-only start: pick who you are or add your name. No accounts, no passwords. */
export function Onboarding() {
  const { data: members = [] } = useMembers();
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const qc = useQueryClient();
  const create = useMutation({
    mutationFn: () => api.createMember({ displayName: name, timezone: localTimezone() }),
    onSuccess: async (m) => {
      setActorId(m.id);
      await qc.invalidateQueries({ queryKey: keys.members });
    },
    onError: (e) => setError(e instanceof ApiError ? e.message : errorMessage(e)),
  });
  return (
    <main className="main" id="main" style={{ maxWidth: 560, margin: "0 auto", paddingTop: "8vh" }}>
      <div className="stack" style={{ alignItems: "center", textAlign: "center" }}>
        <Mascot pose="wave" size={120} label="Pim, Tendly's mascot, waving hello" />
        <h1>Hi, I'm Pim. Welcome to Tendly.</h1>
        <p className="muted">Fewer things to remember. Let's start with what to call you.</p>
      </div>
      <div className="card stack" style={{ marginTop: 24 }}>
        <form
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            setError(null);
            create.mutate();
          }}
        >
          <Field label="Your name" hint="Shown to people you share groups with. It's a name tag, not a password." error={error}>
            {({ id, describedBy, invalid }) => (
              <input id={id} className="input" value={name} onChange={(e) => setName(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} maxLength={40} autoComplete="given-name" autoFocus />
            )}
          </Field>
          <button type="submit" className="btn btn-primary btn-lg" disabled={create.isPending}>
            {create.isPending ? "Setting up…" : "Let's go"}
          </button>
        </form>
        {members.length > 0 && (
          <section aria-labelledby="returning">
            <h2 id="returning" className="section-title">
              Already here?
            </h2>
            <div className="chips">
              {members.map((m) => (
                <button key={m.id} type="button" className="chip" onClick={() => setActorId(m.id)}>
                  I'm {m.displayName}
                </button>
              ))}
            </div>
          </section>
        )}
      </div>
      <p className="small muted" style={{ marginTop: 16, textAlign: "center" }}>
        On a shared computer or home network, anyone who can open Tendly can pick any name. Use remote mode with device pairing if you need real access control.
      </p>
    </main>
  );
}
