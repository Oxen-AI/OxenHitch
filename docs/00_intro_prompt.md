Build a a simple desktop app that allows you to configure inference for your agent through the Oxen.ai gateway. Starting with Claude Desktop, Hitch will configure all your files and give you a simple UI to enter your API key.

What is the best language to do this cross platform? We are going to start with MacOS.

The most annoying part to get this all working manually was:

1) putting Claude desktop into developer mode
2) editing the config to have `"claudeAiImport": { "enabled": true },`
  a) This file was in `~/Library/Application Support/Claude-3p/configLibrary/c2898676-28c3-4ef2-90bb-6a10501a33c1.json` but I don't know if that is consistent across devices, the tool might have to search for it.

The format of the json file is finally:

{
  "claudeAiImport": { "enabled": true },
  "inferenceGatewayBaseUrl": "https://hub.oxen.ai/api/ai",
  "inferenceGatewayApiKey": "__sealed__:v1:API_KEY",
  "inferenceProvider": "gateway",
  "inferenceCredentialKind": "static"
}

The app should have a little status bar in the desktop's top bar, like Docker or Dropbox with a simple dropdown config where you can update the inferenceGatewayBaseUrl & inferenceGatewayApiKey, on install it should search the system and setup the claude configs properly.

Do research on how to set this up programatically so I am not missing anything. The idea is that a non-technical person could set this up, enter their oxen.ai api key, and easily run inference through oxen while using the Claude Desktop App and harness.

I have a local version of oxen running at localhost:3001 that is Anthropic compatible streaming apis and model listing apis, so you can use that for any testing you need.

The design should minimal, beautiful, delightful, and intuitive UI/UX for the app. You are a designer from Figma, Notion, and Apple. This app should be up for Apple Design of the Year Awards.

Ask me any clarifying questions before building, so we come to a shared understanding of how it should work. Save all of the information in docs/01_project_brief.md for future developers and agents to read to get their bearings of what we are building and why.

As this conversation progresses, save any tech stack decisions to docs/02_tech_stack.md and any other important decisions to docs/03_decisions.md for future developers agents to understand why we made the decisions we did.
