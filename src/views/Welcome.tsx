import { useState } from "react";

import { links, type Api, type Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import { Arrow, Field, Notice, Spinner, Yoke } from "../components/ui";
import { RestartPrompt } from "./Home";

/** First run: one field, one button. */
export function Welcome({ api, status }: { api: Api; status: Status }) {
  const [key, setKey] = useState("");
  const [address, setAddress] = useState(status.baseUrl);
  const [custom, setCustom] = useState(false);

  const connect = useAction(() =>
    api.connect({
      apiKey: key,
      baseUrl: custom && address.trim() !== status.baseUrl ? address.trim() : undefined,
    }),
  );
  const ready = key.trim() !== "" && !connect.busy;
  const submit = () => {
    if (ready) void connect.run();
  };

  return (
    <main className="screen welcome">
      <div className="app-tile">
        <Yoke size={30} />
      </div>
      <h1>Run Claude on Oxen</h1>
      <p className="lede">Paste your Oxen API key and Hitch sets up Claude Desktop for you.</p>

      <div className="stack">
        <Field
          label="Oxen API key"
          placeholder="Oxen API key"
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
            label="Gateway address"
            showLabel
            value={address}
            onChange={setAddress}
            onEnter={submit}
          />
        )}
        {connect.error && <Notice>{connect.error}</Notice>}
        <button type="button" className="button primary" disabled={!ready} onClick={submit}>
          {connect.busy ? (
            <>
              <Spinner /> Checking key…
            </>
          ) : (
            "Connect"
          )}
        </button>
      </div>

      <div className="welcome-links">
        <button type="button" className="text-link" onClick={() => void api.open(links.apiKeys)}>
          Get an API key <Arrow />
        </button>
        {!custom && (
          <button type="button" className="text-link quiet" onClick={() => setCustom(true)}>
            Use a different gateway
          </button>
        )}
      </div>

      {status.needsRestart && (
        <RestartPrompt api={api} label="Restart Claude" note="Claude is still running with its old settings." />
      )}
    </main>
  );
}

export function NoClaude({ api }: { api: Api }) {
  return (
    <main className="screen welcome">
      <div className="app-tile">
        <Yoke size={30} />
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
