# Oxen Hitch

A simple desktop app that allows you to run inference for your agent through the Oxen.ai gateway. Starting with Claude Desktop, Hitch will configure all your files and give you a simple UI to enter your API key.

You can find a full project brief to get oriented at docs/01_project_brief.md. Refer to the [Important Files and Folders](#important-files-and-folders) section to learn more about the project.

## Design UI/UX

The design should minimal, beautiful, delightful, and intuitive UI/UX for the app. You are a designer from Figma, Notion, and Apple. This app should be up for Apple Design of the Year Awards. People should comment on how well designed the app is, it's something we take pride in.

## Development Loop

Each time you make a change, do this loop iteration:

1. **Read the task / spec** (and the relevant `*.md` — start at `00-project-brief.md`) and come up with a plan to implement.
2. **Come to a shared understanding** If anything is unclear, ask the user for clarification before implementing.
3. **Write or update a test that encodes the change in behavior** — *before* the code where practical. Make sure the test fails before implementing the feature (red), and passes afterwards (green).
4. **Make the smallest change** that moves toward green.
5. **Prefer writing to the filesystem directly** — don't hold large diffs in conversation.
6. **On failure, fix the root cause**, not the symptom; iterate.
7. **Stop when all checks pass** and the requirement is met — then stop editing.
9. **Commit the change** with a clear, concise message that explains the *why*,
   not just the *what*. Keep commits small and logical — one coherent change each.

When you add a capability, add the test in the same iteration.

## Important Files and Folders

Here are a list of helpful files that you may want to read or write from when working on this project

| File | Load when |
|------|-----------|
| `docs/01_project_brief.md` | Always — orient any session |
| `docs/02_tech_stack.md` | Implementation — the tech stack we are using and why |
| `docs/03_decisions.md` | Implementation — decisions + rationale, append with date after implementing |
| `docs/plans/{feature-name}.md` | Planning — documenting a plan for a specific feature |
