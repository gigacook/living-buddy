import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, errorMessage } from "../lib/api";
import { keys } from "../lib/queries";
import { Mascot } from "../components/Mascot";
import { Field } from "../components/ui";

/** Remote mode: this device must be paired with a code created on the server. */
export function PairDevice() {
  const [code, setCode] = useState("");
  const [error, setError] = useState<string | null>(null);
  const qc = useQueryClient();
  const pair = useMutation({
    mutationFn: () => api.pair(code.trim()),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.session }),
    onError: (e) => setError(errorMessage(e)),
  });
  return (
    <main className="main" id="main" style={{ maxWidth: 520, margin: "0 auto", paddingTop: "10vh" }}>
      <div className="stack" style={{ alignItems: "center", textAlign: "center" }}>
        <Mascot pose="think" size={100} label="Pim thinking" />
        <h1>Pair this device</h1>
        <p className="muted">
          This Tendly server is reachable over the internet, so each device needs a pairing code. Ask whoever runs it to create one with <span className="kbd">tendly device add --name "My phone"</span>.
        </p>
      </div>
      <form
        className="card stack"
        onSubmit={(e) => {
          e.preventDefault();
          setError(null);
          pair.mutate();
        }}
      >
        <Field label="Pairing code" error={error}>
          {({ id, describedBy, invalid }) => (
            <input id={id} className="input" value={code} onChange={(e) => setCode(e.target.value)} aria-describedby={describedBy} aria-invalid={invalid} autoComplete="one-time-code" spellCheck={false} />
          )}
        </Field>
        <button type="submit" className="btn btn-primary" disabled={pair.isPending || !code.trim()}>
          Pair device
        </button>
      </form>
    </main>
  );
}
