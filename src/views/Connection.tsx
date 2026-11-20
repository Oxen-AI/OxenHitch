import { useState } from "react";

import { hostLabel, type Api, type Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import { Check, Field, Notice, Spinner, SubHeader } from "../components/ui";

/** Replace the key, change the host, test it, or take Oxen out of Claude. */
export function Connection(props: { api: Api; status: Status; onDone: () => void }) {
  const { api, status, onDone } = props;
  const [key, setKey] = useState("");
  const currentHost = hostLabel(status.baseUrl);
  const [host, setHost] = useState(currentHost);
  const [answeredBy, setAnsweredBy] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  const newKey = key.trim();
  const newHost = host.trim() !== currentHost ? host.trim() : "";
  const dirty = newKey !== "" || newHost !== "";

  const save = useAction(async () => {
    await api.connect({ apiKey: newKey || undefined, baseUrl: newHost || undefined });
    onDone();
  });
  const test = useAction(async () => {
    setAnsweredBy(null);
    setAnsweredBy(await api.testMessage());
  });
  const remove = useAction(() => api.remove());
  const submit = () => {
    if (dirty && !save.busy) void save.run();
  };

  return (
    <main className="screen sub">
      <SubHeader title="Connection" onBack={onDone} />

      <div className="stack">
        <Field
          label="Oxen API key"
          showLabel
          secret
          placeholder={status.keyHint ? `••••${status.keyHint} — paste a new key to replace it` : "Oxen API key"}
          value={key}
          onChange={setKey}
          onEnter={submit}
        />
        <Field
          label="Host"
          showLabel
          placeholder="hub.oxen.ai"
          value={host}
          onChange={setHost}
          onEnter={submit}
        />
        {save.error && <Notice>{save.error}</Notice>}
        <button type="button" className="button primary" disabled={!dirty || save.busy} onClick={submit}>
          {save.busy ? (
            <>
              <Spinner /> Checking…
            </>
          ) : (
            "Save"
          )}
        </button>
      </div>

      <div className="group">
        <button type="button" className="row row-link" disabled={test.busy} onClick={() => void test.run()}>
          <span className="row-label">Send a test message</span>
          {test.busy && <Spinner />}
          {answeredBy && !test.busy && (
            <span className="row-value good">
              <Check />
              <span>{answeredBy} answered</span>
            </span>
          )}
        </button>
        {test.error && <p className="row-help bad">{test.error}</p>}
      </div>

      <div className="danger">
        {confirming ? (
          <>
            <p className="detail">
              Claude goes back to how it was before Hitch, and your key is forgotten.
            </p>
            <div className="danger-actions">
              <button type="button" className="button secondary compact" onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="button destructive compact"
                disabled={remove.busy}
                onClick={() => void remove.run()}
              >
                {remove.busy ? <Spinner /> : null} Remove
              </button>
            </div>
            {remove.error && <Notice>{remove.error}</Notice>}
          </>
        ) : (
          <button type="button" className="text-link destructive-link" onClick={() => setConfirming(true)}>
            Remove Oxen from Claude…
          </button>
        )}
      </div>
    </main>
  );
}
