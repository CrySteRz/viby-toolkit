# Third-party code in `browser/`

## firefox-agent-bridge

`extension/` is vendored from [1jehuang/firefox-agent-bridge](https://github.com/1jehuang/firefox-agent-bridge)
at `b8b0c146bae4e547b227cca3862df511b43ceabe` (2026-10-05), extension version 0.10.1.
`host.ts` and `install.ts` port its Rust native host and `scripts/build-extensions.py`.
Licence: MIT, Copyright (c) 2026 1jehuang; the full text is `extension/LICENSE`.

Changes from upstream: renamed (native host `viby_browser`, gecko id `browser@viby-toolkit`,
title "Viby Browser"); credential features removed (`secureAutoFill`, auth detection and
notifications); Safari WebSocket relay removed; `notifications`, `webRequest` and `debugger`
permissions dropped; password field values masked in snapshots; the Firefox fallback in
`injectContentScript` fixed (it called itself). To see every change, diff `extension/`
against that commit.

## jcode

`actions.ts` ports the `browser` tool surface — action names, request mapping, key-press
script, error explanations — from [1jehuang/jcode](https://github.com/1jehuang/jcode) at
`a3107fb` (`crates/jcode-app-core/src/tool/browser.rs`, `browser_request.rs`), and
`agents/browser.md` restates its browser handoff contract.

MIT License, Copyright (c) 2025 Jeremy Huang:

> Permission is hereby granted, free of charge, to any person obtaining a copy of this
> software and associated documentation files (the "Software"), to deal in the Software
> without restriction, including without limitation the rights to use, copy, modify, merge,
> publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
> to whom the Software is furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in all copies or
> substantial portions of the Software.
>
> THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
> INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
> PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
> FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
> OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
> DEALINGS IN THE SOFTWARE.
