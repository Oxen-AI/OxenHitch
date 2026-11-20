import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import App from "./App";
import { connectedStatus, createFakeApi, type FakeOptions } from "./lib/fake";

function open(options: FakeOptions = {}) {
  const fake = createFakeApi(options);
  render(<App api={fake.api} />);
  return { ...fake, user: userEvent.setup() };
}

describe("first run", () => {
  it("asks for an API key and nothing else", async () => {
    open();
    expect(await screen.findByRole("heading", { name: "Run Claude on Oxen" })).toBeVisible();
    expect(screen.getByLabelText("Oxen API key")).toBeVisible();
    expect(screen.queryByLabelText("Host")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Connect" })).toBeDisabled();
  });

  it("connects with a pasted key and then asks to restart Claude", async () => {
    const { user, calls } = open();
    await user.type(await screen.findByLabelText("Oxen API key"), "oxen_key_1234");
    await user.click(screen.getByRole("button", { name: "Connect" }));

    expect(await screen.findByRole("heading", { name: "Restart Claude to finish" })).toBeVisible();
    expect(calls).toContain("connect:oxen_key_1234:");

    await user.click(screen.getByRole("button", { name: "Restart Claude" }));
    expect(await screen.findByRole("heading", { name: "Claude is running on Oxen" })).toBeVisible();
    expect(calls).toContain("restartClaude");
  });

  it("offers the key already saved on this Mac, with nothing to paste", async () => {
    const { user, calls } = open({ savedKeys: { "hub.oxen.ai": "9876" } });
    expect(await screen.findByText(/Found your Oxen key ending in 9876/)).toBeVisible();

    const connect = screen.getByRole("button", { name: "Connect" });
    await waitFor(() => expect(connect).toBeEnabled());
    await user.click(connect);

    expect(await screen.findByRole("heading", { name: "Restart Claude to finish" })).toBeVisible();
    expect(calls).toContain("connect::");
    expect(screen.getByText("••••9876")).toBeVisible();
  });

  it("looks for a saved key again when the host changes", async () => {
    const { user } = open({ savedKeys: { "localhost:3001": "4321" } });
    await user.click(await screen.findByRole("button", { name: "Use a different host" }));
    expect(screen.queryByText(/Found your Oxen key/)).not.toBeInTheDocument();

    const host = screen.getByLabelText("Host");
    await user.clear(host);
    await user.type(host, "localhost:3001");

    expect(await screen.findByText(/Found your Oxen key ending in 4321/)).toBeVisible();
    expect(screen.getByRole("button", { name: "Connect" })).toBeEnabled();
  });

  it("connects when Enter is pressed in the key field", async () => {
    const { user, calls } = open();
    await user.type(await screen.findByLabelText("Oxen API key"), "k{Enter}");
    await waitFor(() => expect(calls).toContain("connect:k:"));
  });

  it("explains a rejected key and stays put", async () => {
    const { user } = open();
    await user.type(await screen.findByLabelText("Oxen API key"), "bad");
    await user.click(screen.getByRole("button", { name: "Connect" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("Oxen didn't accept that key");
    expect(screen.getByRole("heading", { name: "Run Claude on Oxen" })).toBeVisible();
  });

  it("opens the page where keys are made", async () => {
    const { user, calls } = open();
    await user.click(await screen.findByRole("button", { name: /Get an API key/ }));
    expect(calls).toContain("open:https://www.oxen.ai/settings/profile");
  });

  it("lets a different host be set, by name alone", async () => {
    const { user, calls } = open();
    await user.click(await screen.findByRole("button", { name: "Use a different host" }));
    const host = screen.getByLabelText("Host");
    expect(host).toHaveValue("hub.oxen.ai");

    await user.clear(host);
    await user.type(host, "localhost:3001");
    await user.type(screen.getByLabelText("Oxen API key"), "k");
    await user.click(screen.getByRole("button", { name: "Connect" }));

    await waitFor(() => expect(calls).toContain("connect:k:localhost:3001"));
    expect(await screen.findByText("Claude picks up Oxen the next time it opens.")).toBeVisible();
  });

  it("points to the Claude download when Claude is not installed", async () => {
    const { user, calls } = open({ status: { claudeInstalled: false, claudeRunning: false } });
    expect(await screen.findByRole("heading", { name: "Install Claude first" })).toBeVisible();
    expect(screen.queryByLabelText("Oxen API key")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Download Claude" }));
    expect(calls).toContain("open:https://claude.com/download");
  });
});

describe("once connected", () => {
  it("shows that Claude is on Oxen and that the gateway is online", async () => {
    open({ status: connectedStatus });
    expect(await screen.findByRole("heading", { name: "Claude is running on Oxen" })).toBeVisible();
    expect(await screen.findByText("Online · 4 models")).toBeVisible();
    expect(screen.getByText("••••1234")).toBeVisible();
  });

  it("shows Claude's own icon from the installed app", async () => {
    const { container } = render(
      <App api={createFakeApi({ status: connectedStatus, claudeIcon: "data:image/png;base64,AAAA" }).api} />,
    );
    await screen.findByRole("heading", { name: "Claude is running on Oxen" });
    await waitFor(() =>
      expect(container.querySelector("img.node-icon")).toHaveAttribute("src", "data:image/png;base64,AAAA"),
    );
  });

  it("falls back to a plain chat bubble when Claude's icon cannot be read", async () => {
    const { container } = render(<App api={createFakeApi({ status: connectedStatus }).api} />);
    await screen.findByRole("heading", { name: "Claude is running on Oxen" });
    expect(container.querySelector("img.node-icon")).toBeNull();
    expect(container.querySelector(".hitch .node svg")).not.toBeNull();
  });

  it("shows that it sees Claude running", async () => {
    open({ status: connectedStatus });
    const row = (await screen.findByText("Claude Desktop")).closest(".row") as HTMLElement;
    expect(within(row).getByText("Running")).toBeVisible();
  });

  it("shows that it sees Claude is not running", async () => {
    open({ status: { ...connectedStatus, claudeRunning: false } });
    const row = (await screen.findByText("Claude Desktop")).closest(".row") as HTMLElement;
    expect(within(row).getByText("Not running")).toBeVisible();
  });

  it("restarts Claude for the user and says so while it happens", async () => {
    const { user, calls } = open({
      status: { ...connectedStatus, needsRestart: true },
      latency: 40,
    });
    await user.click(await screen.findByRole("button", { name: "Restart Claude" }));

    expect(await screen.findByRole("button", { name: "Restarting Claude…" })).toBeDisabled();
    expect(await screen.findByRole("heading", { name: "Claude is running on Oxen" })).toBeVisible();
    expect(calls).toContain("restartClaude");
    expect(screen.queryByRole("button", { name: /Restart/ })).not.toBeInTheDocument();
  });

  it("says so while it opens Claude", async () => {
    const { user } = open({ status: { ...connectedStatus, claudeRunning: false }, latency: 40 });
    await user.click(await screen.findByRole("button", { name: "Open Claude" }));
    expect(await screen.findByRole("button", { name: "Opening Claude…" })).toBeDisabled();
    expect(await screen.findByRole("heading", { name: "Claude is running on Oxen" })).toBeVisible();
  });

  it("offers to open Claude when it is not running", async () => {
    const { user, calls } = open({ status: { ...connectedStatus, claudeRunning: false } });
    expect(await screen.findByRole("heading", { name: "Claude is set to use Oxen" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Open Claude" }));
    expect(calls).toContain("restartClaude");
  });

  it("switches back to Anthropic and on again", async () => {
    const { user, calls } = open({ status: { ...connectedStatus, claudeRunning: false } });
    const toggle = await screen.findByRole("switch", { name: "Run Claude on Oxen" });
    expect(toggle).toBeChecked();

    await user.click(toggle);
    expect(await screen.findByRole("heading", { name: "Claude is using Anthropic" })).toBeVisible();
    expect(calls).toContain("setRouting:false");

    await user.click(screen.getByRole("switch", { name: "Run Claude on Oxen" }));
    expect(await screen.findByRole("heading", { name: "Claude is set to use Oxen" })).toBeVisible();
  });

  it("says so when the key stops working", async () => {
    open({ status: connectedStatus, check: { state: "invalid_key" } });
    expect(await screen.findByText("Key not accepted")).toBeVisible();
  });

  it("asks for the key again when the saved one cannot be read", async () => {
    open({ status: connectedStatus, keyUnreadable: true });
    expect(await screen.findByText("Key needed")).toBeVisible();
    expect(screen.getByText(/Enter your key again under API key/)).toBeVisible();
    expect(screen.queryByText("Checking…")).not.toBeInTheDocument();
  });

  it("says so when the gateway cannot be reached", async () => {
    open({
      status: connectedStatus,
      check: { state: "unreachable", message: "Could not reach hub.oxen.ai." },
    });
    expect(await screen.findByText("Can't reach hub.oxen.ai")).toBeVisible();
  });

  it("offers to reconnect when Claude's setup was changed elsewhere", async () => {
    const { user, calls } = open({ status: { ...connectedStatus, link: "drifted" } });
    expect(await screen.findByRole("heading", { name: "Claude's setup changed" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Reconnect" }));
    await waitFor(() => expect(calls).toContain("setRouting:true"));
  });

  it("warns when an organization manages Claude's settings", async () => {
    open({ status: { ...connectedStatus, managed: true } });
    expect(await screen.findByText(/Your organization manages Claude/)).toBeVisible();
  });

  it("turns on opening at login", async () => {
    const { user, calls } = open({ status: connectedStatus });
    const toggle = await screen.findByRole("switch", { name: "Open at login" });
    expect(toggle).not.toBeChecked();
    await user.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(calls).toContain("setAutostart:true");
  });

  it("follows changes made outside the window", async () => {
    const { update } = open({ status: connectedStatus });
    await screen.findByRole("heading", { name: "Claude is running on Oxen" });
    update({ link: "paused" });
    expect(await screen.findByRole("heading", { name: "Claude is using Anthropic" })).toBeVisible();
  });
});

describe("choosing models", () => {
  async function openModels(options: FakeOptions = { status: connectedStatus }) {
    const view = open(options);
    await view.user.click(await screen.findByRole("button", { name: /^Models/ }));
    await screen.findByRole("heading", { name: "Models" });
    return view;
  }

  it("starts with every Oxen model shown", async () => {
    await openModels();
    expect(screen.getByRole("radio", { name: /All models/ })).toBeChecked();
  });

  it("says how many models that is on the home screen", async () => {
    open({ status: connectedStatus });
    const row = await screen.findByRole("button", { name: /^Models/ });
    expect(await within(row).findByText("All 4")).toBeVisible();
  });

  it("says how many models Oxen offers", async () => {
    await openModels();
    expect(await screen.findByText("4 models from Oxen")).toBeVisible();
  });

  it("refreshes the list from Oxen and says when nothing changed", async () => {
    const { user, calls } = await openModels();
    const before = calls.filter((call) => call === "check").length;

    await user.click(screen.getByRole("button", { name: "Refresh models" }));

    expect(await screen.findByText("Up to date")).toBeVisible();
    expect(calls.filter((call) => call === "check").length).toBe(before + 1);
    expect(screen.getByRole("checkbox", { name: /Claude Sonnet 5.5/ })).toBeVisible();
  });

  it("says when a refresh changed Claude's list", async () => {
    const view = open({ status: { ...connectedStatus, claudeRunning: false }, checkUpdatesClaude: true });
    await view.user.click(await screen.findByRole("button", { name: /^Models/ }));
    view.update({ claudeRunning: true });

    await view.user.click(await screen.findByRole("button", { name: "Refresh models" }));

    expect(await screen.findByText("Updated. Restart Claude to see the changes.")).toBeVisible();
  });

  it("keeps the list on screen while refreshing", async () => {
    const { user } = await openModels({ status: connectedStatus, latency: 40 });
    await screen.findByRole("checkbox", { name: /Claude Sonnet 5.5/ });

    await user.click(screen.getByRole("button", { name: "Refresh models" }));

    expect(screen.getByRole("checkbox", { name: /Claude Sonnet 5.5/ })).toBeVisible();
    expect(await screen.findByText("Up to date")).toBeVisible();
  });

  it("saves the chosen models with the first pick as the default", async () => {
    const { user, calls } = await openModels();
    await user.click(screen.getByRole("checkbox", { name: /Claude Sonnet 5.5/ }));
    await user.click(screen.getByRole("checkbox", { name: /llama-4-maverick/ }));
    expect(within(screen.getByTestId("model-claude-sonnet-5-5")).getByText("Default")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Done" }));

    await waitFor(() => expect(calls).toContain("setModels:claude-sonnet-5-5,llama-4-maverick"));
    expect(await screen.findByText("Claude Sonnet 5.5 + 1")).toBeVisible();
  });

  it("lets another chosen model become the default", async () => {
    const { user, calls } = await openModels({
      status: { ...connectedStatus, models: ["claude-opus-5-5", "claude-haiku-5-5"] },
    });
    await user.click(screen.getByRole("button", { name: "Make Claude Haiku 5.5 the default" }));
    await user.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(calls).toContain("setModels:claude-haiku-5-5,claude-opus-5-5"));
  });

  it("goes back to showing every model", async () => {
    const { user, calls } = await openModels({
      status: { ...connectedStatus, models: ["claude-opus-5-5"] },
    });
    await user.click(screen.getByRole("radio", { name: /All models/ }));
    await user.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(calls).toContain("setModels:"));
  });

  it("does not save when nothing changed", async () => {
    const { user, calls } = await openModels();
    await user.click(screen.getByRole("button", { name: "Done" }));
    await screen.findByRole("heading", { name: /Claude is/ });
    expect(calls.some((call) => call.startsWith("setModels"))).toBe(false);
  });
});

describe("connection settings", () => {
  async function openConnection() {
    const view = open({ status: connectedStatus });
    await view.user.click(await screen.findByRole("button", { name: /^API key/ }));
    await screen.findByRole("heading", { name: "Connection" });
    return view;
  }

  async function openHost() {
    const view = open({ status: connectedStatus });
    await view.user.click(await screen.findByRole("button", { name: /^Host/ }));
    await screen.findByRole("heading", { name: "Connection" });
    return view;
  }

  it("replaces the key", async () => {
    const { user, calls } = await openConnection();
    // Nothing to save until something changes.
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();

    await user.type(screen.getByLabelText("API key"), "new_key_9876");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(calls).toContain("connect:new_key_9876:"));
    expect(await screen.findByText("••••9876")).toBeVisible();
  });

  it("changes the host without retyping the key", async () => {
    const { user, calls } = await openConnection();
    const host = screen.getByLabelText("Host");
    expect(host).toHaveValue("hub.oxen.ai");

    await user.clear(host);
    await user.type(host, "localhost:3001");
    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(calls).toContain("connect::localhost:3001"));
    // Back home, the new host shows once Claude has been restarted.
    await user.click(await screen.findByRole("button", { name: "Restart Claude" }));
    expect(await screen.findByText("localhost:3001")).toBeVisible();
  });

  it("opens on the key from the API key row", async () => {
    await openConnection();
    expect(screen.getByLabelText("API key")).toHaveFocus();
  });

  it("opens on the host from a Host row on the home screen", async () => {
    const { user } = open({ status: connectedStatus });
    const row = await screen.findByRole("button", { name: /^Host/ });
    expect(within(row).getByText("hub.oxen.ai")).toBeVisible();

    await user.click(row);
    expect(await screen.findByRole("heading", { name: "Connection" })).toBeVisible();
    expect(screen.getByLabelText("Host")).toHaveFocus();
  });

  it("says which saved key a different host will use", async () => {
    const view = open({ status: connectedStatus, savedKeys: { "localhost:3001": "4321" } });
    await view.user.click(await screen.findByRole("button", { name: /^Host/ }));
    const host = await screen.findByLabelText("Host");
    expect(screen.queryByText(/Found your Oxen key/)).not.toBeInTheDocument();

    await view.user.clear(host);
    await view.user.type(host, "localhost:3001");
    expect(await screen.findByText("Found your Oxen key ending in 4321 for localhost:3001.")).toBeVisible();

    await view.user.click(screen.getByRole("button", { name: "Save" }));
    expect(await screen.findByText("••••4321")).toBeVisible();
  });

  it("sends a test message", async () => {
    const { user } = await openConnection();
    await user.click(screen.getByRole("button", { name: "Send a test message" }));
    expect(await screen.findByText("claude-opus-5-5 answered through hub.oxen.ai")).toBeVisible();
  });

  it("shows the saved key as filled in, and asks for a new one once the field is entered", async () => {
    const { user } = await openHost();
    const key = screen.getByLabelText("API key");
    expect(key).toHaveAttribute("placeholder", "••••••••1234");

    await user.click(key);
    expect(key).toHaveAttribute("placeholder", "Paste a new key");
  });

  it("says nothing extra until there is something to say", async () => {
    await openHost();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("offers Save only while there is a change to save", async () => {
    const { user } = await openConnection();
    const host = screen.getByLabelText("Host");
    await user.type(host, "x");
    expect(screen.getByRole("button", { name: "Save" })).toBeEnabled();

    await user.type(host, "{Backspace}");
    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });

  it("removes Oxen from Claude only after confirming", async () => {
    const { user, calls } = await openConnection();
    await user.click(screen.getByRole("button", { name: "Remove Oxen from Claude…" }));
    expect(calls).not.toContain("remove");

    await user.click(screen.getByRole("button", { name: "Remove" }));
    expect(await screen.findByRole("heading", { name: "Run Claude on Oxen" })).toBeVisible();
    expect(calls).toContain("remove");
  });
});
