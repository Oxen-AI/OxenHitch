import { useState } from "react";

import { hostLabel, hostOf, type Api, type Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import { useSavedKey } from "../lib/useSavedKey";
import { Check, Notice, RowField, Spinner, SubHeader } from "../components/ui";

/** Which field a row on the home screen leads to. */
export type ConnectionFocus = "key" | "host";

/** Replace the key, change the host, test it, or take Oxen out of Claude. */
export function Connection(props: { api: Api; status: Status; focus: ConnectionFocus; onDone: () => void }) {
  const { api, status, focus, onDone } = props;
  const [key, setKey] = useState("");
  const currentHost = hostLabel(status.baseUrl);
  const [host, setHost] = useState(currentHost);
  const [answeredBy, setAnsweredBy] = useState<string | null>(null);
  const [confirming, setConfirming] = useState(false);

  const newKey = key.trim();
  const newHost = host.trim() !== currentHost ? host.trim() : "";
  const dirty = newKey !== "" || newHost !== "";
  // Each host keeps its own key, so moving to one used before needs nothing pasted.
  const saved = useSavedKey(api, newHost);

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

  const testedHost = hostOf(status.baseUrl);
  // Only speak up when a change needs explaining.
  const caption =
    newHost === "" || newKey !== ""
      ? null
      : saved
        ? `Found your Oxen key ending in ${saved} for ${newHost}.`
        : `Your current key will be used for ${newHost}.`;

  return (
    <main className="screen sub">
      <SubHeader
        title="Connection"
        onBack={onDone}
        action={
          dirty ? (
            <button type="button" className="button primary pill" disabled={save.busy} onClick={submit}>
              {save.busy ? <Spinner /> : "Save"}
            </button>
          ) : null
        }
      />

      <div className="stack">
        <div className="group">
          <RowField
            label="Host"
            placeholder="hub.oxen.ai"
            autoFocus={focus === "host"}
            value={host}
            onChange={setHost}
            onEnter={submit}
          />
          <RowField
            label="API key"
            secret
            autoFocus={focus === "key"}
            saved={status.keyHint ? `••••••••${status.keyHint}` : undefined}
            placeholder={status.keyHint ? "Paste a new key" : "Paste your key"}
            value={key}
            onChange={setKey}
            onEnter={submit}
          />
        </div>
        {caption && (
          <p className="caption" role="status">
            {caption}
          </p>
        )}
        {save.error && <Notice>{save.error}</Notice>}
      </div>

      <div className="group">
        <div className="row">
          <span className="row-label">Test message</span>
          <button
            type="button"
            className="button secondary pill"
            aria-label="Send a test message"
            disabled={test.busy}
            onClick={() => void test.run()}
          >
            {test.busy ? <Spinner /> : answeredBy ? "Send again" : "Send"}
          </button>
        </div>
        {answeredBy && !test.busy && (
          <p className="row-result good">
            <Check />
            <span>{`${answeredBy} answered through ${testedHost}`}</span>
          </p>
        )}
        {test.error && !test.busy && <p className="row-result bad">{test.error}</p>}
      </div>

      <div className="group">
        {confirming ? (
          <div className="confirm">
            <p className="detail">Claude goes back to how it was before Hitch. Your Oxen key stays on this Mac.</p>
            <div className="confirm-actions">
              <button type="button" className="button secondary pill" onClick={() => setConfirming(false)}>
                Cancel
              </button>
              <button
                type="button"
                className="button destructive pill"
                disabled={remove.busy}
                onClick={() => void remove.run()}
              >
                {remove.busy ? <Spinner /> : null} Remove
              </button>
            </div>
            {remove.error && <Notice>{remove.error}</Notice>}
          </div>
        ) : (
          <button type="button" className="row row-link row-destructive" onClick={() => setConfirming(true)}>
            Remove Oxen from Claude…
          </button>
        )}
      </div>
    </main>
  );
}
