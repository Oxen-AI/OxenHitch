import { useState } from "react";

import type { Gateway } from "../App";
import type { Api, Model, Status } from "../lib/api";
import { useAction } from "../lib/useAction";
import { Check, Notice, Spinner, SubHeader } from "../components/ui";

/** More models than this and a search field appears. */
const SEARCH_FROM = 8;

/** Pick which models Claude shows. The first pick is the default. */
export function Models(props: { api: Api; status: Status; gateway: Gateway; onDone: () => void }) {
  const { api, status, gateway, onDone } = props;
  const [picked, setPicked] = useState<string[]>(status.models);
  const [query, setQuery] = useState("");

  const changed = picked.join("\n") !== status.models.join("\n");
  const save = useAction(async () => {
    if (changed) await api.setModels(picked);
    onDone();
  });

  const offered: Model[] = gateway.state === "ok" ? gateway.models : [];
  // A model picked earlier that the gateway no longer lists still shows, so it can be unpicked.
  const gone: Model[] = picked
    .filter((id) => !offered.some((model) => model.id === id))
    .map((id) => ({ id, name: null }));
  const all = [...gone, ...offered];
  const needle = query.trim().toLowerCase();
  const shown = needle
    ? all.filter((model) => `${model.name ?? ""} ${model.id}`.toLowerCase().includes(needle))
    : all;

  const toggle = (id: string) =>
    setPicked((now) => (now.includes(id) ? now.filter((other) => other !== id) : [...now, id]));
  const makeDefault = (id: string) => setPicked((now) => [id, ...now.filter((other) => other !== id)]);

  return (
    <main className="screen sub">
      <SubHeader
        title="Models"
        onBack={onDone}
        action={
          <button type="button" className="text-link strong" disabled={save.busy} onClick={() => void save.run()}>
            {save.busy ? <Spinner /> : "Done"}
          </button>
        }
      />
      {save.error && <Notice>{save.error}</Notice>}

      <div className="group">
        <button
          type="button"
          role="radio"
          aria-checked={picked.length === 0}
          className="row row-link choice"
          onClick={() => setPicked([])}
        >
          <span className="tick" data-on={picked.length === 0}>
            <Check />
          </span>
          <span className="choice-text">
            <span className="row-label">Automatic</span>
            <span className="choice-note">Claude shows the Claude models Oxen offers.</span>
          </span>
        </button>
      </div>

      <p className="section-title">Or choose which to show</p>

      {gateway.state === "checking" && (
        <p className="empty">
          <Spinner /> Loading models…
        </p>
      )}
      {gateway.state !== "checking" && gateway.state !== "ok" && all.length === 0 && (
        <p className="empty">Models can't be loaded right now.</p>
      )}

      {all.length > SEARCH_FROM && (
        <input
          className="search"
          type="search"
          placeholder="Search models"
          aria-label="Search models"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      )}

      {shown.length > 0 && (
        <div className="group scroll">
          {shown.map((model) => {
            const label = model.name ?? model.id;
            const on = picked.includes(model.id);
            const isDefault = picked[0] === model.id;
            return (
              <div className="row model" key={model.id} data-testid={`model-${model.id}`}>
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={on}
                  aria-label={label}
                  className="choice"
                  onClick={() => toggle(model.id)}
                >
                  <span className="tick" data-on={on}>
                    <Check />
                  </span>
                  <span className="choice-text">
                    <span className="row-label">{label}</span>
                    {model.name && <span className="choice-note mono">{model.id}</span>}
                  </span>
                </button>
                {isDefault ? (
                  <span className="pill">Default</span>
                ) : on ? (
                  <button
                    type="button"
                    className="text-link make-default"
                    aria-label={`Make ${label} the default`}
                    onClick={() => makeDefault(model.id)}
                  >
                    Make default
                  </button>
                ) : null}
              </div>
            );
          })}
        </div>
      )}
      {all.length > 0 && shown.length === 0 && <p className="empty">No models match “{query}”.</p>}
    </main>
  );
}
