/** Save the provider settings and discover LiteLLM models when the list is blank.
 * DISCOVER [L1]: blank optional model IDs request the catalog on Save.
 * APPLY [L2]: apply the returned Claude alias profile.
 */
import { useState } from "react";

import { hostLabel, links, type Api, type Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import { useSavedKey } from "../lib/useSavedKey";
import { Arrow, Field, Notice, OxenMark, Spinner } from "../components/ui";
import { RestartPrompt } from "./Home";

/** First run: one field, one button. */
export function Welcome({ api, status }: { api: Api; status: Status }) {
  const [key, setKey] = useState("");
  const currentHost = hostLabel(status.baseUrl);
  const [host, setHost] = useState("http://127.0.0.1:4000");
  const [custom, setCustom] = useState(false);
  const [modelText, setModelText] = useState("");

  // A key the Oxen CLI (or an earlier Hitch) saved for this host means nothing to paste.
  const lookupHost = custom ? host.trim() : currentHost;
  const saved = useSavedKey(api, lookupHost);
  const modelIds = modelText.split(/[\n,]/).map((value) => value.trim()).filter(Boolean);

  const connect = useAction(() =>
    api.connect({
      apiKey: key,
      baseUrl: custom ? host.trim() : undefined,
      customEndpoint: custom,
      modelIds: custom ? modelIds : undefined,
    }),
  );
  const ready = (custom ? Boolean(host.trim()) : key.trim() !== "" || saved !== null) && !connect.busy;
  const submit = () => {
    if (ready) void connect.run();
  };

  return (
    <main className="screen welcome">
      <div className="app-tile">
        <OxenMark size={28} />
      </div>
      <h1>{custom ? "Use your endpoint" : "Run Claude on Oxen"}</h1>
      <p className="lede">
        {custom
          ? "Hitch can find the models on your LiteLLM endpoint."
          : saved
          ? "Hitch sets up Claude Desktop for you. Your Oxen key is already on this Mac."
          : "Paste your Oxen API key and Hitch sets up Claude Desktop for you."}
      </p>

      <div className="stack">
        <Field
          label={custom ? "API key (optional)" : "Oxen API key"}
          placeholder={custom ? "Provider API key (optional)" : saved ? "Paste a different key (optional)" : "Oxen API key"}
          value={key}
          secret
          autoFocus
          onChange={(value) => {
            setKey(value);
            connect.clearError();
          }}
          onEnter={submit}
        />
        {custom && (
          <Field
            label="Endpoint URL"
            showLabel
            placeholder="http://127.0.0.1:4000"
            value={host}
            onChange={(value) => {
              setHost(value);
              connect.clearError();
            }}
            onEnter={submit}
          />
        )}
        {custom && (
          <Field
            label="Model names (optional)"
            showLabel
            placeholder="Leave blank to discover LiteLLM models"
            value={modelText}
            onChange={setModelText}
            onEnter={submit}
          />
        )}
        {saved && key.trim() === "" && (
          <Notice tone="info">
            {`Found your ${custom ? "provider" : "Oxen"} key ending in ${saved} for ${lookupHost}.`}
          </Notice>
        )}
        {connect.error && <Notice>{connect.error}</Notice>}
        <button type="button" className="button primary" disabled={!ready} onClick={submit}>
          {connect.busy ? (
            <>
              <Spinner /> {custom ? (modelIds.length ? "Saving…" : "Finding models…") : "Checking key…"}
            </>
          ) : (
            custom ? "Save endpoint" : "Connect"
          )}
        </button>
      </div>

      <div className="welcome-links">
        {!custom && (
          <>
            <button type="button" className="text-link" onClick={() => void api.open(links.apiKeys)}>
              Get an API key <Arrow />
            </button>
            <button type="button" className="text-link quiet" onClick={() => { setKey(""); setCustom(true); }}>
              Use a different endpoint
            </button>
          </>
        )}
        {custom && (
          <button type="button" className="text-link quiet" onClick={() => { setKey(""); setCustom(false); }}>
            Use Oxen
          </button>
        )}
      </div>

      {status.needsRestart && (
        <RestartPrompt api={api} note="Claude is still running with its old settings." />
      )}
    </main>
  );
}

export function NoClaude({ api }: { api: Api }) {
  return (
    <main className="screen welcome">
      <div className="app-tile">
        <OxenMark size={28} />
      </div>
      <h1>Install Claude first</h1>
      <p className="lede">
        Hitch connects the Claude desktop app to Oxen. Install Claude and this window will pick up
        from there.
      </p>
      <div className="stack">
        <button type="button" className="button primary" onClick={() => void api.open(links.claudeDownload)}>
          Download Claude
        </button>
      </div>
    </main>
  );
}
