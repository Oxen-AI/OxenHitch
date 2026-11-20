import type { ReactNode } from "react";

import oxenMark from "../assets/oxen-x-white.svg";

export function Switch(props: {
  label: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      type="button"
      role="switch"
      className="switch"
      aria-label={props.label}
      aria-checked={props.checked}
      disabled={props.disabled}
      onClick={() => props.onChange(!props.checked)}
    >
      <span className="switch-knob" />
    </button>
  );
}

export function Spinner() {
  return <span className="spinner" aria-hidden="true" />;
}

/** The Oxen mark, in white. It only ever sits on a dark tile. */
export function OxenMark({ size = 22 }: { size?: number }) {
  return <img className="oxen-mark" src={oxenMark} height={size} alt="" draggable={false} />;
}

/** A plain chat bubble standing in for Claude. */
export function Bubble({ size = 22 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" aria-hidden="true">
      <path
        d="M12 4.500c-4.400 0-8 3-8 6.800 0 2.100 1.100 4 2.900 5.200l-.600 3 3.300-1.700c.800.200 1.600.300 2.400.300 4.400 0 8-3 8-6.800s-3.600-6.800-8-6.800Z"
        stroke="currentColor"
        strokeWidth="1.700"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Chevron() {
  return (
    <svg className="chevron" width="7" height="12" viewBox="0 0 7 12" fill="none" aria-hidden="true">
      <path d="m1 1 5 5-5 5" stroke="currentColor" strokeWidth="1.600" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function Check() {
  return (
    <svg width="10" height="8" viewBox="0 0 10 8" fill="none" aria-hidden="true">
      <path d="m1 4.200 2.600 2.600L9 1.200" stroke="currentColor" strokeWidth="1.800" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

export function Refresh() {
  return (
    <svg width="13" height="13" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <path
        d="M12 7a5 5 0 1 1-1.600-3.660M12 2.400v3h-3"
        stroke="currentColor"
        strokeWidth="1.500"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

export function Arrow() {
  return (
    <svg width="9" height="9" viewBox="0 0 9 9" fill="none" aria-hidden="true">
      <path d="M2 7 7 2M3 2h4v4" stroke="currentColor" strokeWidth="1.400" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

/** The title bar of a sub-screen: back on the left, an optional action on the right. */
export function SubHeader(props: { title: string; onBack: () => void; action?: ReactNode }) {
  return (
    <header className="subheader">
      <button type="button" className="back" aria-label="Back" onClick={props.onBack}>
        <svg width="8" height="13" viewBox="0 0 8 13" fill="none" aria-hidden="true">
          <path d="M7 1 1.500 6.500 7 12" stroke="currentColor" strokeWidth="1.800" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      <h1>{props.title}</h1>
      <div className="subheader-action">{props.action}</div>
    </header>
  );
}

export function Field(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
  autoFocus?: boolean;
  /** Show the label above the field. Otherwise it is only read by screen readers. */
  showLabel?: boolean;
  onEnter?: () => void;
}) {
  return (
    <label className="field">
      <span className={props.showLabel ? "field-label" : "sr-only"}>{props.label}</span>
      <input
        type={props.secret ? "password" : "text"}
        value={props.value}
        placeholder={props.placeholder}
        autoFocus={props.autoFocus}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(event) => props.onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") props.onEnter?.();
        }}
      />
    </label>
  );
}

/**
 * A field that is a row in a group: its name on the left, its value typed on the right.
 * Used where settings are edited in place, so the screen reads like the list it came from.
 */
export function RowField(props: {
  label: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
  secret?: boolean;
  autoFocus?: boolean;
  onEnter?: () => void;
}) {
  return (
    <label className="row row-field">
      <span className="row-label">{props.label}</span>
      <input
        type={props.secret ? "password" : "text"}
        value={props.value}
        placeholder={props.placeholder}
        autoFocus={props.autoFocus}
        autoComplete="off"
        autoCorrect="off"
        autoCapitalize="off"
        spellCheck={false}
        onChange={(event) => props.onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter") props.onEnter?.();
        }}
      />
    </label>
  );
}

export function Notice(props: { tone?: "error" | "info"; children: ReactNode }) {
  const tone = props.tone ?? "error";
  return (
    <p className={`notice notice-${tone}`} role={tone === "error" ? "alert" : "status"}>
      {props.children}
    </p>
  );
}
