# Terax Architecture Report

Date: 2026-07-13

Baseline: 460657a, main

Scope: repository architecture only. No feature implementation, refactor, dependency, or runtime behavior change.

## Executive conclusion

Terax is already more than a terminal emulator. It is a Tauri desktop development environment with:

- React 19 and TypeScript frontend
- Rust host boundary
- portable-pty interactive shells
- xterm.js renderer pooling
- tabs, spaces, persisted workspaces, and split panes
- AI chat tools
- managed Claude Code terminals
- Codex, Claude Code, and Gemini CLI activity detection
- an existing direct write path to a terminal leaf

The proposed future send_to_terminal capability does not need a broker, mailbox, queue, or persistence layer. Core transport already exists:

Frontend leaf id -> frontend Session -> PtySession.write -> raw Tauri invoke -> PtyState -> PTY writer

Main missing abstraction: target identity. Current code has several partial registries, but no single terminal directory connecting user-visible name, space, tab, pane leaf, frontend session, managed agent, and backend PTY id.

Recommended direction:

1. Keep pane leaf id as canonical frontend runtime terminal identity.
2. Separate display label from unique addressable terminal name.
3. Add a terminal-domain directory above writeToSession, outside App and AI modules.
4. Move backend writer lookup into a reusable PtyState method before adding any external API.
5. Add external control only after identity, lifecycle, and authorization contracts are stable.

## Architecture audit findings

### High: terminal identity ownership is fragmented

Runtime identity exists in several places:

- tab and pane topology in useTabs
- module-global terminal sessions keyed by leaf id
- App-owned terminalRefs keyed by leaf id
- renderer slots keyed by slot id and associated with leaf id
- managed agents keyed by leaf id with tab id and chat session id
- Rust PtyState keyed by PTY id

Evidence:

- [TerminalTab and Tab types, src/modules/tabs/lib/useTabs.ts:29](../../src/modules/tabs/lib/useTabs.ts#L29)
- [PaneNode, src/modules/terminal/lib/panes.ts:13](../../src/modules/terminal/lib/panes.ts#L13)
- [frontend session creation, src/modules/terminal/lib/useTerminalSession.ts:426](../../src/modules/terminal/lib/useTerminalSession.ts#L426)
- [App terminalRefs, src/app/App.tsx:163](../../src/app/App.tsx#L163)
- [managed agent identity, src/modules/agents/store/managedAgentsStore.ts:7](../../src/modules/agents/store/managedAgentsStore.ts#L7)
- [backend PtyState, src-tauri/src/modules/pty/mod.rs:18](../../src-tauri/src/modules/pty/mod.rs#L18)

Why it matters: adding name lookup independently in App, AI tooling, or Rust would create competing registries and stale mappings.

Direction: one terminal-domain directory, explicit lifecycle registration, and explicit mapping to the backend PTY id.

### High: current direct-write success is only local acceptance

writeToSession returns true when a frontend Session exists and is not exited. Actual pty.write is launched without awaiting its Tauri invoke. A backend error can therefore occur after the caller receives true.

Evidence:

- [writeToSession, src/modules/terminal/lib/useTerminalSession.ts:150](../../src/modules/terminal/lib/useTerminalSession.ts#L150)
- [raw frontend PTY write, src/modules/terminal/lib/pty-bridge.ts:62](../../src/modules/terminal/lib/pty-bridge.ts#L62)
- [backend pty_write errors, src-tauri/src/modules/pty/mod.rs:98](../../src-tauri/src/modules/pty/mod.rs#L98)
- [existing send_to_agent use, src/modules/ai/tools/agent.ts:63](../../src/modules/ai/tools/agent.ts#L63)

Why it matters: agent collaboration needs a precise contract. With no queue or persistence, success can mean only one of:

- target resolved
- bytes accepted by frontend session
- bytes written to PTY writer
- target program consumed input

Only the first three are observable by Terax. The last is impossible without agent-specific acknowledgment.

Direction: define send_to_terminal as best-effort PTY injection and return the strongest observable result, preferably completion of backend writer.write_all.

### Medium: user-visible names exist, but are tab labels, not terminal addresses

TerminalTab already has customTitle. It persists through space serialization and survives cwd changes. It is optional, not unique, tab-scoped, and cannot distinguish panes inside a split tab.

Evidence:

- [customTitle, src/modules/tabs/lib/useTabs.ts:29](../../src/modules/tabs/lib/useTabs.ts#L29)
- [serialized customTitle, src/modules/spaces/lib/serialize.ts:18](../../src/modules/spaces/lib/serialize.ts#L18)
- [tab label behavior tests, src/modules/tabs/lib/tabLabel.test.ts:16](../../src/modules/tabs/lib/tabLabel.test.ts#L16)

Direction: keep customTitle as presentation. Add a separate addressable name contract, including uniqueness scope and pane behavior.

### Medium: App is already a broad orchestration boundary

App owns tab orchestration, leaf cleanup, terminal handle maps, workspace switching, AI live bridge, pane actions, shell injection helpers, and most cross-module composition.

Evidence:

- [App, src/app/App.tsx:106](../../src/app/App.tsx#L106)
- [live leaf cleanup, src/app/App.tsx:342](../../src/app/App.tsx#L342)
- [WorkspaceSurface composition, src/app/components/WorkspaceSurface.tsx:42](../../src/app/components/WorkspaceSurface.tsx#L42)

Why it matters: placing send_to_terminal in App would make terminal addressing dependent on React component lifetime and increase coupling.

Direction: App may publish topology changes to a terminal-domain service, but should not own target resolution or delivery.

### Medium: existing PTY architecture documentation has stale cardinality

docs/architecture/pty-shell-integration.md says one terminal tab maps to one PTY session. Current code supports split panes. TerminalTab owns PaneNode, PaneTreeView creates one TerminalPane per leaf, and each leaf creates its own Session and PTY.

Evidence:

- [stale statement, docs/architecture/pty-shell-integration.md](pty-shell-integration.md)
- [TerminalTab paneTree, src/modules/tabs/lib/useTabs.ts:29](../../src/modules/tabs/lib/useTabs.ts#L29)
- [recursive pane rendering, src/modules/terminal/PaneTreeView.tsx:28](../../src/modules/terminal/PaneTreeView.tsx#L28)
- [per-leaf hook, src/modules/terminal/TerminalPane.tsx:40](../../src/modules/terminal/TerminalPane.tsx#L40)

Correct model: one terminal tab owns one or more leaves; each live leaf owns one frontend Session and normally one backend PTY session.

### Note: major future building blocks already exist

- Split panes already exist.
- Named tab labels already exist.
- Direct leaf writes already exist.
- Managed agent registry already maps chat session -> leaf -> tab.
- send_to_agent already injects approved input into a managed Claude Code PTY.
- Agent activity detection already supports Claude Code, Codex, and Gemini CLI through OSC markers.

Evidence:

- [split operations, src/modules/tabs/lib/useTabs.ts:1000](../../src/modules/tabs/lib/useTabs.ts#L1000)
- [writeToSession, src/modules/terminal/lib/useTerminalSession.ts:150](../../src/modules/terminal/lib/useTerminalSession.ts#L150)
- [managed agents, src/modules/agents/store/managedAgentsStore.ts:7](../../src/modules/agents/store/managedAgentsStore.ts#L7)
- [send_to_agent, src/modules/ai/tools/agent.ts:63](../../src/modules/ai/tools/agent.ts#L63)
- [agent detector, src-tauri/src/modules/pty/agent_detect.rs:38](../../src-tauri/src/modules/pty/agent_detect.rs#L38)

## 1. Overall architecture

### Technology shape

Frontend:

- Vite
- React 19
- TypeScript
- xterm.js 6
- xterm Fit, Search, Serialize, WebLinks, and WebGL addons
- CodeMirror 6
- Zustand stores
- Tauri JavaScript APIs and plugins

Backend:

- Tauri 2
- Rust 2021
- portable-pty 0.9
- native OS APIs through Rust and windows-sys
- Tauri managed state
- synchronous worker threads for PTY and process IO
- Tokio runtime feature used where Tauri async commands need it

Dependency evidence:

- [package.json](../../package.json)
- [src-tauri/Cargo.toml](../../src-tauri/Cargo.toml)
- [TERAX.md](../../TERAX.md)

### Application entry

Rust:

1. src-tauri/src/main.rs calls terax_lib::run.
2. lib.rs constructs Tauri Builder.
3. Plugins and managed states are registered.
4. Eighty custom Tauri commands are registered.
5. Tauri runs until app exit.

Evidence:

- [Rust main, src-tauri/src/main.rs:4](../../src-tauri/src/main.rs#L4)
- [Tauri builder, src-tauri/src/lib.rs:114](../../src-tauri/src/lib.rs#L114)

Frontend:

1. main.tsx loads xterm and global CSS.
2. pty_close_all reaps PTYs from a previous webview load.
3. launch directory is loaded before first render.
4. React renders App.
5. hidden native window is shown after paint.

Evidence:

- [frontend main, src/main.tsx:1](../../src/main.tsx#L1)
- [Tauri window config, src-tauri/tauri.conf.json](../../src-tauri/tauri.conf.json)

### Frontend module architecture

App is the composition root. Main feature boundaries live under src/modules:

- tabs: tab union, active tab, pane topology mutations
- spaces: workspace metadata and persisted tab topology
- terminal: xterm, renderer pool, frontend sessions, pane views, PTY bridge
- workspace: Local and WSL environment selection
- agents: detected and managed agent state
- ai: chat, tools, model providers, local agent bridge
- editor, explorer, git-history, source-control, preview, markdown, LSP
- settings and theme

WorkspaceSurface keeps feature stacks mounted in parallel and hides inactive stacks. TerminalStack separately keeps live terminal tabs mounted and toggles visibility. This preserves background output and avoids recreating terminal sessions on tab switches.

Evidence:

- [WorkspaceSurface, src/app/components/WorkspaceSurface.tsx:42](../../src/app/components/WorkspaceSurface.tsx#L42)
- [TerminalStack, src/modules/terminal/TerminalStack.tsx:27](../../src/modules/terminal/TerminalStack.tsx#L27)

### Backend module architecture

src-tauri/src/modules owns host operations:

- pty: interactive shell lifecycle and stream transport
- shell: one-shot, persistent logical shell sessions, background processes
- proc: process helpers and Windows Job Objects
- fs: tree, file, mutation, watch, search, grep
- git: source-control operations
- lsp: language-server process host and JSON-RPC transport
- workspace: Local/WSL path handling and authorization registry
- net: AI HTTP proxy and local model probe
- secrets: OS keychain or Linux protected file fallback
- history: shell history
- agent: coding-agent hook installation

lib.rs manages:

- PtyState
- ShellState
- SecretsState
- FsWatchState
- HistoryState
- LspState
- ContentSearchState
- WorkspaceRegistry
- LaunchDir

Evidence:

- [managed states, src-tauri/src/lib.rs:176](../../src-tauri/src/lib.rs#L176)
- [module declarations, src-tauri/src/modules/mod.rs](../../src-tauri/src/modules/mod.rs)

### Logical versus OS process model

Repository docs use “two-process model” for the security boundary:

- webview frontend
- Rust backend

That is a logical ownership model, not a literal complete process count. Code also creates:

- one child shell process per PTY leaf
- shell descendants, contained by Windows Job Object when available
- LSP server child processes
- one-shot command children
- background process children
- native webview/settings windows

Each PTY also creates reader, flusher, and waiter threads. Close can create a detached drop thread.

Evidence:

- [two-process guide](two-process-model.md)
- [PTY spawn, src-tauri/src/modules/pty/session.rs:102](../../src-tauri/src/modules/pty/session.rs#L102)
- [LSP session spawn, src-tauri/src/modules/lsp/session.rs:69](../../src-tauri/src/modules/lsp/session.rs#L69)
- [shell subsystem, src-tauri/src/modules/shell/mod.rs:41](../../src-tauri/src/modules/shell/mod.rs#L41)

Exact OS process counts remain platform and webview-runtime dependent. Repository code does not define that count.

### Terminal rendering pipeline

The live terminal path is:

1. `TerminalPane` binds a pane leaf to a frontend session and a pooled xterm renderer.
2. xterm input callbacks pass keyboard and paste data through the terminal adapter to `writeToSession` and then the Tauri `pty_write` command.
3. Rust writes the byte body to the portable-pty writer.
4. The Rust reader and flusher threads return output through the PTY data channel.
5. `deliverPtyBytes` writes live bytes into xterm. If the renderer is dormant, it retains output in the session ring buffer and replays it when a renderer is rebound.
6. xterm performs terminal emulation and DOM/WebGL rendering. The Fit addon calculates rows and columns, then `pty_resize` synchronizes those dimensions with the native PTY.

The PTY lifecycle surrounds this pipeline: a leaf creates the session, mounting or hibernating a renderer does not close it, and leaf disposal or application shutdown closes it.

The renderer pool is deliberately separate from PTY lifetime. A leaf can keep its PTY and frontend session alive while its scarce renderer slot is hibernated, serialized, and later restored. This distinction matters for future terminal addressing: direct writes must target the session or PTY, not the currently mounted xterm instance.

Evidence:

- [TerminalPane, src/modules/terminal/TerminalPane.tsx:40](../../src/modules/terminal/TerminalPane.tsx#L40)
- [renderer pool slot creation, src/modules/terminal/lib/rendererPool.ts:198](../../src/modules/terminal/lib/rendererPool.ts#L198)
- [frontend PTY delivery, src/modules/terminal/lib/useTerminalSession.ts:477](../../src/modules/terminal/lib/useTerminalSession.ts#L477)
- [PTY bridge open and channels, src/modules/terminal/lib/pty-bridge.ts:18](../../src/modules/terminal/lib/pty-bridge.ts#L18)

### Application architecture diagram

~~~mermaid
flowchart TB
  subgraph UI["Webview frontend: React + TypeScript"]
    Main["main.tsx"]
    App["App composition root"]
    Tabs["Tabs + Spaces"]
    Surface["WorkspaceSurface"]
    TermStack["TerminalStack"]
    PaneTree["PaneTreeView"]
    TermPane["TerminalPane"]
    Session["useTerminalSession"]
    Pool["xterm rendererPool"]
    Bridge["pty-bridge"]
    AI["AI + managed agents"]
  end

  subgraph Host["Rust Tauri host"]
    Commands["80 Tauri commands"]
    PtyState["PtyState"]
    ShellState["ShellState"]
    HostModules["fs / git / lsp / net / secrets / history / workspace"]
  end

  subgraph OS["OS resources"]
    Pty["portable-pty native PTY"]
    Shell["shell process tree"]
    Lsp["LSP processes"]
    Files["filesystem / git / keychain / network"]
  end

  Main --> App
  App --> Tabs
  App --> Surface
  Surface --> TermStack
  TermStack --> PaneTree
  PaneTree --> TermPane
  TermPane --> Session
  Session <--> Pool
  Session <--> Bridge
  AI --> Session
  Bridge <--> Commands
  Commands --> PtyState
  Commands --> ShellState
  Commands --> HostModules
  PtyState <--> Pty
  Pty <--> Shell
  HostModules <--> Lsp
  HostModules <--> Files
~~~

## 2. PTY architecture

### Creation

Frontend creation begins when a terminal leaf mounts:

1. PaneTreeView renders TerminalPane for a leaf.
2. TerminalPane calls useTerminalSession with leafId.
3. useTerminalSession calls ensureSession.
4. When DOM container attaches, attachSession starts openPtyWithRetry.
5. openPtyForSession calls openPty.
6. pty-bridge creates separate data and exit Tauri Channels.
7. invoke("pty_open") sends size, cwd, workspace, shell, block mode, and channels.

Evidence:

- [PaneTreeView, src/modules/terminal/PaneTreeView.tsx:28](../../src/modules/terminal/PaneTreeView.tsx#L28)
- [useTerminalSession, src/modules/terminal/lib/useTerminalSession.ts:822](../../src/modules/terminal/lib/useTerminalSession.ts#L822)
- [attachSession, src/modules/terminal/lib/useTerminalSession.ts:674](../../src/modules/terminal/lib/useTerminalSession.ts#L674)
- [openPtyForSession, src/modules/terminal/lib/useTerminalSession.ts:522](../../src/modules/terminal/lib/useTerminalSession.ts#L522)
- [openPty, src/modules/terminal/lib/pty-bridge.ts:18](../../src/modules/terminal/lib/pty-bridge.ts#L18)

Rust creation:

1. pty_open normalizes workspace and cwd.
2. PtyState allocates a monotonically increasing u32 id.
3. spawn_blocking calls session::spawn.
4. native_pty_system selects portable-pty native implementation.
5. openpty creates master/slave pair at initial size.
6. shell_init::build_command selects Unix or Windows/WSL command.
7. slave.spawn_command starts the shell.
8. slave is dropped.
9. master reader clone and writer are captured.
10. Session stores child killer, writer, master, shell PID, exit flag, and Windows Job Object.
11. Session is inserted into PtyState.sessions.

Evidence:

- [pty_open, src-tauri/src/modules/pty/mod.rs:42](../../src-tauri/src/modules/pty/mod.rs#L42)
- [session::spawn, src-tauri/src/modules/pty/session.rs:102](../../src-tauri/src/modules/pty/session.rs#L102)
- [shell command selection, src-tauri/src/modules/pty/shell_init.rs:52](../../src-tauri/src/modules/pty/shell_init.rs#L52)

### portable-pty and ConPTY

The repository does not directly create ConPTY. It calls portable_pty::native_pty_system. On Windows, portable-pty supplies native pseudoconsole integration. Terax adds Windows-specific lifecycle protections:

- global CONPTY_LIFECYCLE_LOCK serializes create and close
- Windows Job Object kills shell descendants on close
- Session field drop order kills process tree before master drop
- ClosePseudoConsole-capable drop runs on detached thread

Evidence:

- [portable-pty imports, src-tauri/src/modules/pty/session.rs:7](../../src-tauri/src/modules/pty/session.rs#L7)
- [ConPTY lifecycle lock, src-tauri/src/modules/pty/session.rs:68](../../src-tauri/src/modules/pty/session.rs#L68)
- [Session field order, src-tauri/src/modules/pty/session.rs:33](../../src-tauri/src/modules/pty/session.rs#L33)
- [Windows Job Object creation, src-tauri/src/modules/proc/job.rs:26](../../src-tauri/src/modules/proc/job.rs#L26)

Exact portable-pty internal ConPTY behavior is outside this repository and requires upstream 0.9 source inspection before changing low-level pseudoconsole assumptions.

### Writer and stdin path

Interactive keyboard:

1. xterm term.onData produces a string.
2. rendererPool resolves current leaf adapter.
3. adapter writeToPty calls PtySession.write.
4. pty-bridge UTF-8 encodes data.
5. invoke("pty_write", raw bytes) includes x-pty-id header.
6. Rust parses header and raw InvokeBody.
7. PtyState finds Session.
8. Session.writer Mutex is locked.
9. write_all sends bytes to PTY master input.

Evidence:

- [xterm onData, src/modules/terminal/lib/rendererPool.ts:303](../../src/modules/terminal/lib/rendererPool.ts#L303)
- [adapter write, src/modules/terminal/lib/useTerminalSession.ts:370](../../src/modules/terminal/lib/useTerminalSession.ts#L370)
- [raw bridge write, src/modules/terminal/lib/pty-bridge.ts:62](../../src/modules/terminal/lib/pty-bridge.ts#L62)
- [Rust writer, src-tauri/src/modules/pty/mod.rs:98](../../src-tauri/src/modules/pty/mod.rs#L98)

Programmatic input follows the same path:

- TerminalPaneHandle.write
- writeToSession
- existing injectIntoActivePty
- existing send_to_agent

Evidence:

- [TerminalPaneHandle, src/modules/terminal/TerminalPane.tsx:75](../../src/modules/terminal/TerminalPane.tsx#L75)
- [writeToSession, src/modules/terminal/lib/useTerminalSession.ts:150](../../src/modules/terminal/lib/useTerminalSession.ts#L150)
- [injectIntoActivePty, src/modules/ai/lib/useAiLiveBridge.ts:94](../../src/modules/ai/lib/useAiLiveBridge.ts#L94)
- [send_to_agent, src/modules/ai/tools/agent.ts:63](../../src/modules/ai/tools/agent.ts#L63)

Input arriving before PTY open is buffered in pendingInput with a fixed cap, then flushed when open completes.

Evidence:

- [queuePendingInput, src/modules/terminal/lib/useTerminalSession.ts:145](../../src/modules/terminal/lib/useTerminalSession.ts#L145)
- [pending flush, src/modules/terminal/lib/useTerminalSession.ts:674](../../src/modules/terminal/lib/useTerminalSession.ts#L674)

### Reader and stdout path

session::spawn creates three threads:

Reader:

- reads up to 16 KiB per call from master reader
- feeds AgentDetector
- emits terax:agent-signal on transitions
- feeds DaFilter
- writes DA replies back through Session.writer when required
- appends filtered bytes to shared pending buffer
- caps pending output at 4 MiB
- replaces overflow with terminal reset plus notice

Flusher:

- waits on condition variable
- coalesces for 4 ms
- sends binary Response chunks through data Channel

Waiter:

- waits for child exit
- ensures reader reaches EOF, with Windows bounded wait
- sends final pending tail
- marks flusher done
- sends exit code through exit Channel
- removes Session from PtyState and drops it

Evidence:

- [thread implementation, src-tauri/src/modules/pty/session.rs:172](../../src-tauri/src/modules/pty/session.rs#L172)
- [buffer constants, src-tauri/src/modules/pty/session.rs:18](../../src-tauri/src/modules/pty/session.rs#L18)

Frontend output:

1. Channel<ArrayBuffer> receives raw output.
2. openPtyForSession calls deliverPtyBytes(leafId, bytes).
3. If leaf has live renderer slot, xterm.write parses and renders bytes.
4. Otherwise DormantRing buffers bytes until a renderer slot is rebound.

Evidence:

- [channel handlers, src/modules/terminal/lib/pty-bridge.ts:27](../../src/modules/terminal/lib/pty-bridge.ts#L27)
- [deliverPtyBytes, src/modules/terminal/lib/useTerminalSession.ts:477](../../src/modules/terminal/lib/useTerminalSession.ts#L477)
- [DormantRing, src/modules/terminal/lib/dormantRing.ts:25](../../src/modules/terminal/lib/dormantRing.ts#L25)

### Resize

ResizeObserver and FitAddon determine xterm rows/columns. Resizes are debounced before PtySession.resize invokes pty_resize. Rust finds Session and calls MasterPty.resize.

Evidence:

- [renderer binding and fit, src/modules/terminal/lib/rendererPool.ts:428](../../src/modules/terminal/lib/rendererPool.ts#L428)
- [frontend resize, src/modules/terminal/lib/pty-bridge.ts:63](../../src/modules/terminal/lib/pty-bridge.ts#L63)
- [backend resize, src-tauri/src/modules/pty/mod.rs:137](../../src-tauri/src/modules/pty/mod.rs#L137)

### Destruction

Frontend leaf removal:

1. App detects leaf absent from current pane trees.
2. disposeSession marks Session disposed.
3. renderer slot is disposed or detached.
4. PtySession.close invokes pty_close.
5. frontend maps and waiters are cleared.

Rust explicit close:

1. pty_close removes Session from PtyState.
2. child killer is called.
3. detached terax-pty-drop-id thread calls drop_session.
4. Windows drop takes CONPTY_LIFECYCLE_LOCK.
5. Session Drop kills again best-effort.
6. fields drop in safe order.

Natural child exit:

1. waiter sends final output and exit code.
2. waiter removes Session from PtyState.
3. drop_session releases resources.
4. frontend marks shellExited and disables stdin.

Webview reload:

- main.tsx calls pty_close_all before rendering tabs.

Evidence:

- [frontend disposal, src/modules/terminal/lib/useTerminalSession.ts:786](../../src/modules/terminal/lib/useTerminalSession.ts#L786)
- [pty_close, src-tauri/src/modules/pty/mod.rs:171](../../src-tauri/src/modules/pty/mod.rs#L171)
- [Session Drop, src-tauri/src/modules/pty/session.rs:57](../../src-tauri/src/modules/pty/session.rs#L57)
- [pty_close_all, src-tauri/src/modules/pty/mod.rs:283](../../src-tauri/src/modules/pty/mod.rs#L283)
- [boot cleanup, src/main.tsx:21](../../src/main.tsx#L21)

### PTY lifecycle diagram

~~~mermaid
sequenceDiagram
  participant Leaf as Terminal leaf
  participant FS as Frontend Session
  participant IPC as Tauri IPC
  participant PS as PtyState
  participant PP as portable-pty
  participant Child as Shell process

  Leaf->>FS: mount leafId
  FS->>IPC: pty_open + data/exit Channels
  IPC->>PS: allocate PTY id
  PS->>PP: native_pty_system.openpty
  PP->>Child: spawn_command
  Child-->>PP: stdout/stderr PTY bytes
  PP-->>FS: reader -> flusher -> data Channel
  FS-->>Leaf: xterm.write or DormantRing
  Leaf->>FS: keyboard/programmatic input
  FS->>IPC: raw pty_write + x-pty-id
  IPC->>PP: writer.write_all
  Leaf->>IPC: pty_resize
  IPC->>PP: MasterPty.resize
  alt explicit close
    Leaf->>IPC: pty_close
    IPC->>Child: kill
  else natural exit
    Child-->>FS: final data then exit code
  end
  PS->>PS: remove Session
  PS->>PP: detached drop_session
~~~

## 3. Terminal model

### Identity layers

| Identity | Type | Owner | Lifetime | Meaning |
| --- | --- | --- | --- | --- |
| Space id | string | spaces store | persisted | workspace grouping |
| Tab id | number | useTabs | current frontend runtime | any tab kind |
| Pane split id | number | PaneNode | current frontend runtime | layout node |
| Leaf id | number | PaneNode/useTerminalSession | current frontend runtime | canonical terminal pane identity |
| Renderer slot id | number | rendererPool | frontend runtime | pooled xterm instance |
| PTY id | u32 | Rust PtyState | backend runtime | interactive PTY session |
| Shell PID | u32 | Rust Session | child lifetime | shell process identity |
| Chat session id | string | AI session store | persisted AI context | chat identity |
| Managed agent mapping | leafId + tabId + sessionId | managedAgentsStore | frontend runtime | one managed coding agent |

Tab ids, split ids, and leaf ids share useTabs allocation. Hydration assigns fresh runtime ids. They are not stable external addresses.

Evidence:

- [cold terminal allocation, src/modules/tabs/lib/useTabs.ts:200](../../src/modules/tabs/lib/useTabs.ts#L200)
- [split allocation, src/modules/tabs/lib/useTabs.ts:1000](../../src/modules/tabs/lib/useTabs.ts#L1000)
- [fresh hydration ids, src/modules/spaces/lib/serialize.ts:220](../../src/modules/spaces/lib/serialize.ts#L220)
- [PTY monotonic ids, src-tauri/src/modules/pty/mod.rs:18](../../src-tauri/src/modules/pty/mod.rs#L18)

### Tabs

Tab is a discriminated union:

- terminal
- editor
- preview
- markdown
- AI diff
- git diff
- git history
- git commit file diff

TerminalTab contains:

- id
- spaceId
- title
- cwd
- paneTree
- activeLeafId
- block mode
- private flag
- customTitle

Evidence:

- [Tab union, src/modules/tabs/lib/useTabs.ts:117](../../src/modules/tabs/lib/useTabs.ts#L117)
- [TerminalTab, src/modules/tabs/lib/useTabs.ts:29](../../src/modules/tabs/lib/useTabs.ts#L29)

### Panes

Panes are implemented.

PaneNode is recursive:

- leaf: id, optional slotId, optional cwd
- split: id, row/column direction, children

PaneTreeView recursively renders ResizablePanelGroup. Each leaf owns one TerminalPane. Current implementation limits a tab to a configured maximum and blocks splitting block-mode terminals.

Evidence:

- [PaneNode, src/modules/terminal/lib/panes.ts:13](../../src/modules/terminal/lib/panes.ts#L13)
- [PaneTreeView, src/modules/terminal/PaneTreeView.tsx:28](../../src/modules/terminal/PaneTreeView.tsx#L28)
- [split mutation, src/modules/tabs/lib/useTabs.ts:1000](../../src/modules/tabs/lib/useTabs.ts#L1000)

### Spaces and workspace model

SpaceMeta contains:

- string id
- name
- root
- WorkspaceEnv
- optional color
- timestamps

SpaceState persists:

- serialized tabs
- active tab index

WorkspaceEnv distinguishes Local and WSL distro. WorkspaceRegistry in Rust is a security registry of authorized roots, not the same object as frontend SpaceMeta.

Evidence:

- [SpaceMeta, src/modules/spaces/lib/store.ts:5](../../src/modules/spaces/lib/store.ts#L5)
- [space persistence, src/modules/spaces/lib/useSpacePersistence.ts:18](../../src/modules/spaces/lib/useSpacePersistence.ts#L18)
- [workspace environment, src/modules/workspace/env.ts:1](../../src/modules/workspace/env.ts#L1)
- [WorkspaceRegistry, src-tauri/src/modules/workspace.rs:20](../../src-tauri/src/modules/workspace.rs#L20)

### Session model

There are three distinct session concepts:

1. Frontend terminal Session
   - keyed by leaf id
   - stores PtySession, cwd, renderer state, pending input/output, callbacks, visibility, block state

2. Backend PTY Session
   - keyed by PTY id
   - stores master, writer, child killer, shell PID, exit flag, Windows Job Object

3. AI chat session
   - keyed by string session id
   - maps to a managed agent through managedAgentsStore when one is spawned

These must remain distinct in future naming work.

### Terminal lifecycle diagram

~~~mermaid
stateDiagram-v2
  [*] --> PersistedColdTab: hydrate space
  [*] --> LiveTab: create tab
  PersistedColdTab --> LeafMounted: activate/render
  LiveTab --> LeafMounted
  LeafMounted --> FrontendSession: ensureSession
  FrontendSession --> PtyOpening: attachSession
  PtyOpening --> LivePty: pty_open success
  PtyOpening --> SpawnFailed: retry exhausted
  SpawnFailed --> PtyOpening: Enter retry
  LivePty --> VisibleSlot: visible leaf
  LivePty --> HiddenStreaming: hidden leaf
  VisibleSlot --> HiddenStreaming: hide/park/release
  HiddenStreaming --> VisibleSlot: acquire/replay
  LivePty --> ShellExited: child exit
  ShellExited --> Disposed: leaf removal
  VisibleSlot --> Disposed: leaf removal
  HiddenStreaming --> Disposed: leaf removal
  Disposed --> [*]
~~~

## 4. State management

### Frontend state

React-local:

- useTabs owns tabs and active tab
- App owns orchestration refs and transient UI state
- component state owns dialogs, search, layout handles, and visibility

Zustand:

- spaces
- workspace environment
- settings/preferences
- AI chat and agents
- managed coding agents
- other feature stores

Tauri plugin store:

- spaces: terax-spaces.json
- preferences and settings
- custom themes
- AI agents/snippets metadata

Module-global terminal runtime:

- sessions Map keyed by leaf id
- renderer slot pool
- ready waiters and activity maps

Evidence:

- [useTabs, src/modules/tabs/lib/useTabs.ts:247](../../src/modules/tabs/lib/useTabs.ts#L247)
- [spaces store, src/modules/spaces/lib/useSpaces.ts:23](../../src/modules/spaces/lib/useSpaces.ts#L23)
- [space disk store, src/modules/spaces/lib/store.ts:21](../../src/modules/spaces/lib/store.ts#L21)
- [terminal runtime, src/modules/terminal/lib/useTerminalSession.ts:426](../../src/modules/terminal/lib/useTerminalSession.ts#L426)
- [renderer pool, src/modules/terminal/lib/rendererPool.ts:22](../../src/modules/terminal/lib/rendererPool.ts#L22)

### Backend state

Tauri managed states use lock-protected maps and counters:

- PtyState: PTY sessions
- ShellState: persistent logical sessions and background processes
- LspState: LSP processes
- FsWatchState: native watcher
- HistoryState
- SecretsState
- ContentSearchState
- WorkspaceRegistry
- LaunchDir

Evidence:

- [state registration, src-tauri/src/lib.rs:176](../../src-tauri/src/lib.rs#L176)

### Synchronization and event flow

Frontend topology is authoritative for tabs, spaces, pane trees, active leaf, and labels.

Backend is authoritative for:

- PTY existence
- OS process and pipe handles
- shell exit
- filesystem operations
- workspace authorization

Synchronization mechanisms:

- invoke request/response for commands
- Channel for high-volume or streaming responses
- Tauri events for broadcast and cross-window state
- plugin-store persistence for restore
- OSC 7 and OSC 133/777 to derive terminal cwd, command state, and agent state
- React callbacks for leaf cwd and exit updates

No transactional synchronization joins frontend tab topology and backend PTY registry. Cleanup is idempotent and tolerant of unknown ids.

### State flow diagram

~~~mermaid
flowchart LR
  Disk["Tauri plugin store"] --> Spaces["Spaces Zustand store"]
  Spaces --> Tabs["useTabs React state"]
  Tabs --> Tree["Pane tree + active leaf"]
  Tree --> Sessions["Frontend Session map"]
  Sessions --> Pool["Renderer slot pool"]
  Sessions --> Bridge["PtySession bridge"]
  Bridge --> Rust["Rust PtyState"]
  Rust --> OS["PTY + shell"]
  OS --> Rust
  Rust -->|data/exit Channels| Sessions
  Sessions -->|cwd/exit callbacks| Tabs
  Tabs -->|debounced serialization| Disk
  Sessions --> Managed["Managed agent Zustand store"]
  Managed --> AI["AI tool context"]
  AI -->|writeToSession| Sessions
~~~

## 5. Communication

### Existing communication paths

#### Tauri invoke request/response

Used for normal frontend-to-Rust commands. Payloads are JSON-serialized except pty_write, which sends raw bytes and an x-pty-id header.

#### Tauri Channels

Long-lived callback channels:

- PTY data: Channel<Response>
- PTY exit: Channel<i32>
- LSP message: Channel<Response>
- LSP exit: Channel<LspExit>
- AI HTTP stream: Channel<AiStreamEvent>

Evidence:

- [PTY bridge, src/modules/terminal/lib/pty-bridge.ts:27](../../src/modules/terminal/lib/pty-bridge.ts#L27)
- [LSP transport, src/modules/lsp/lib/transport.ts:47](../../src/modules/lsp/lib/transport.ts#L47)
- [AI proxy stream, src/modules/ai/lib/proxyFetch.ts:97](../../src/modules/ai/lib/proxyFetch.ts#L97)

#### Rust-emitted Tauri events

- fs:changed: filesystem watcher updates
- fs:file-written: editor/theme conflict synchronization
- terax:agent-signal: PTY-detected agent lifecycle
- terax:settings-tab: settings window deep-link

Evidence:

- [fs:changed, src-tauri/src/modules/fs/watch.rs:137](../../src-tauri/src/modules/fs/watch.rs#L137)
- [fs:file-written, src-tauri/src/modules/fs/file.rs:128](../../src-tauri/src/modules/fs/file.rs#L128)
- [terax:agent-signal, src-tauri/src/modules/pty/session.rs:16](../../src-tauri/src/modules/pty/session.rs#L16)
- [settings event, src-tauri/src/lib.rs:35](../../src-tauri/src/lib.rs#L35)

#### Frontend-emitted Tauri events

Used mainly for cross-window synchronization:

- terax://prefs-changed
- terax://ai-keys-changed
- terax://custom-themes-changed
- terax://theme-edit
- terax://ai-agents-changed
- terax://ai-snippets-changed

Evidence:

- [settings events, src/modules/settings/store.ts:348](../../src/modules/settings/store.ts#L348)
- [theme events, src/modules/theme/customThemes.ts:7](../../src/modules/theme/customThemes.ts#L7)
- [agent metadata event, src/modules/ai/store/agentsStore.ts:12](../../src/modules/ai/store/agentsStore.ts#L12)
- [snippet metadata event, src/modules/ai/store/snippetsStore.ts:10](../../src/modules/ai/store/snippetsStore.ts#L10)

#### Frontend-only events

- terax:ai-attach-file CustomEvent
- terax:toggle-block-input CustomEvent
- React callbacks and Zustand subscriptions

These do not cross into Rust.

#### PTY control sequences

PTY output contains semantic control messages:

- OSC 7: cwd
- OSC 133: prompt/command lifecycle
- OSC 52: clipboard
- OSC 777: agent lifecycle
- DA queries: filtered and answered by Rust

This is an implicit PTY-to-application communication path.

### End-to-end terminal communication

~~~mermaid
sequenceDiagram
  participant X as xterm / caller
  participant S as frontend Session
  participant T as Tauri IPC
  participant R as Rust PtyState
  participant P as PTY master
  participant C as shell / agent CLI

  X->>S: onData or writeToSession
  S->>T: pty_write raw bytes
  T->>R: resolve x-pty-id
  R->>P: writer.write_all
  P->>C: stdin
  C-->>P: PTY output
  P-->>R: reader bytes
  R-->>S: data Channel
  S-->>X: xterm.write
  opt agent marker
    C-->>R: OSC 777
    R-->>X: terax:agent-signal
  end
~~~

### Every registered Tauri command

Registration source: [src-tauri/src/lib.rs:192](../../src-tauri/src/lib.rs#L192). Total: 80.

#### PTY, 9

| Command | Purpose |
| --- | --- |
| pty_open | Create interactive PTY, spawn shell, return PTY id, attach data and exit channels. |
| pty_write | Write raw request body to PTY selected by x-pty-id header. |
| pty_resize | Resize PTY master rows and columns. |
| pty_close | Remove, kill, and asynchronously drop one PTY session. |
| pty_close_all | Drain, kill, and asynchronously drop all PTY sessions. |
| pty_has_foreground_process | Check whether shell has child processes. |
| pty_has_foreground_job | Check whether foreground job owns tty, with Windows child-process fallback. |
| pty_shell_name | Return detected default shell name. |
| pty_list_shells | Return available shells. |

Module: [src-tauri/src/modules/pty/mod.rs](../../src-tauri/src/modules/pty/mod.rs)

#### Filesystem, 18

| Command | Purpose |
| --- | --- |
| list_subdirs | List immediate subdirectories for cwd navigation. |
| fs_read_dir | Read one directory with metadata and optional git decorations. |
| fs_read_file | Read bounded text file and classify too-large or binary input. |
| fs_write_file | Atomically write file, preserve permissions, emit fs:file-written. |
| fs_stat | Return file metadata. |
| fs_canonicalize | Resolve path to canonical application path. |
| fs_create_file | Create empty file without overwrite. |
| fs_create_dir | Create directory chain without overwriting existing target. |
| fs_rename | Rename or move without overwriting destination. |
| fs_delete | Delete file or recursively delete directory. |
| fs_copy | Copy external files/directories into workspace destination. |
| fs_watch_add | Add authorized paths to native filesystem watcher. |
| fs_watch_remove | Remove paths from native filesystem watcher. |
| fs_search | Fuzzy-rank filesystem entries. |
| fs_list_files | Recursively list files with bounds. |
| fs_grep | Search file contents. |
| fs_grep_interactive | Search contents with interactive cancellation/state. |
| fs_glob | Match workspace paths by glob. |

Modules:

- [src-tauri/src/modules/fs/tree.rs](../../src-tauri/src/modules/fs/tree.rs)
- [src-tauri/src/modules/fs/file.rs](../../src-tauri/src/modules/fs/file.rs)
- [src-tauri/src/modules/fs/mutate.rs](../../src-tauri/src/modules/fs/mutate.rs)
- [src-tauri/src/modules/fs/watch.rs](../../src-tauri/src/modules/fs/watch.rs)
- [src-tauri/src/modules/fs/search.rs](../../src-tauri/src/modules/fs/search.rs)
- [src-tauri/src/modules/fs/grep.rs](../../src-tauri/src/modules/fs/grep.rs)

#### LSP, 6

| Command | Purpose |
| --- | --- |
| lsp_detect | Detect configured/available language server. |
| lsp_host_pid | Return Terax host PID for lifecycle diagnostics. |
| lsp_resolve_root | Resolve nearest project root within authorized scope. |
| lsp_spawn | Spawn language server and attach message/exit channels. |
| lsp_send | Send framed JSON-RPC payload to server stdin. |
| lsp_kill | Kill and remove language-server session. |

Module: [src-tauri/src/modules/lsp/mod.rs](../../src-tauri/src/modules/lsp/mod.rs)

#### Git, 19

| Command | Purpose |
| --- | --- |
| git_resolve_repo | Resolve authorized repository root. |
| git_panel_snapshot | Return combined repository/status snapshot for UI. |
| git_status | Return working-tree status. |
| git_diff | Return diff metadata/content for requested scope. |
| git_diff_content | Return file diff content. |
| git_stage | Stage paths. |
| git_unstage | Unstage paths. |
| git_discard | Discard selected working-tree changes. |
| git_commit | Create commit. |
| git_fetch | Fetch remotes. |
| git_pull_ff_only | Fast-forward-only pull. |
| git_push | Push branch with upstream handling. |
| git_log | Return commit history. |
| git_show_commit | Return one commit details. |
| git_commit_files | Return files changed by commit. |
| git_commit_file_diff | Return one file diff for commit. |
| git_remote_url | Return repository remote URL. |
| git_list_branches | List local/remote branch information. |
| git_checkout_branch | Checkout requested branch. |

Module: [src-tauri/src/modules/git/commands.rs](../../src-tauri/src/modules/git/commands.rs)

#### Shell, 8

| Command | Purpose |
| --- | --- |
| shell_run_command | Run bounded one-shot command for AI tools, capture stdout/stderr. |
| shell_session_open | Create persistent logical agent shell session state. |
| shell_session_run | Run command using persistent session cwd/environment. |
| shell_session_close | Remove persistent logical shell session. |
| shell_bg_spawn | Spawn long-running background process and return handle. |
| shell_bg_logs | Read bounded background logs from offset. |
| shell_bg_kill | Kill background process. |
| shell_bg_list | List background process handles and status. |

Module: [src-tauri/src/modules/shell/mod.rs](../../src-tauri/src/modules/shell/mod.rs)

Important distinction: shell_session_* is not the interactive terminal PTY path.

#### Workspace and WSL, 5

| Command | Purpose |
| --- | --- |
| wsl_list_distros | List installed WSL distributions. |
| wsl_default_distro | Return default WSL distribution. |
| wsl_home | Resolve home directory inside WSL distribution. |
| workspace_authorize | Add user-approved workspace root to authorization registry. |
| workspace_current_dir | Return current startup/workspace directory. |

Module: [src-tauri/src/modules/workspace.rs](../../src-tauri/src/modules/workspace.rs)

#### Root/window, 2

| Command | Purpose |
| --- | --- |
| get_launch_dir | Return and consume CLI launch directory. |
| open_settings_window | Open/focus settings webview and optional tab deep-link. |

Module: [src-tauri/src/lib.rs:14](../../src-tauri/src/lib.rs#L14)

#### Agent hooks, 2

| Command | Purpose |
| --- | --- |
| agent_enable_hooks | Install/update supported coding-agent lifecycle hooks. |
| agent_hooks_status | Report whether agent hooks are installed. |

Module: [src-tauri/src/modules/agent.rs](../../src-tauri/src/modules/agent.rs)

#### Secrets, 4

| Command | Purpose |
| --- | --- |
| secrets_get | Read one provider secret. |
| secrets_set | Store one provider secret. |
| secrets_delete | Delete one provider secret. |
| secrets_get_all | Read all configured provider secrets. |

Module: [src-tauri/src/modules/secrets.rs](../../src-tauri/src/modules/secrets.rs)

#### Network, 3

| Command | Purpose |
| --- | --- |
| lm_ping | Probe local model endpoint through guarded Rust network path. |
| ai_http_request | Execute non-streaming AI HTTP request through SSRF guard. |
| ai_http_stream | Execute streaming AI HTTP request and emit AiStreamEvent channel items. |

Module: [src-tauri/src/modules/net.rs](../../src-tauri/src/modules/net.rs)

#### History, 4

| Command | Purpose |
| --- | --- |
| history_suggest | Return ranked shell-history suggestions. |
| history_commands | Return command candidates/history command data. |
| history_record | Record command execution metadata. |
| history_list | Return history entries. |

Module: [src-tauri/src/modules/history/mod.rs](../../src-tauri/src/modules/history/mod.rs)

### Tauri communication diagram

~~~mermaid
flowchart TB
  FE["Frontend"]
  Invoke["invoke request/response"]
  Raw["raw invoke: pty_write"]
  Channel["Tauri Channels"]
  Events["Tauri events"]
  Plugins["Tauri plugins"]
  Rust["Rust command handlers"]
  State["Managed Rust state"]
  PTY["PTY"]

  FE --> Invoke --> Rust
  FE --> Raw --> Rust
  Rust --> State
  State <--> PTY
  Rust --> Channel --> FE
  Rust --> Events --> FE
  FE --> Events
  FE <--> Plugins
~~~

## 6. Extension points for send_to_terminal

### Existing relevant paths

Active terminal injection:

- useAiLiveBridge resolves active tab and active leaf
- terminalRefs finds TerminalPaneHandle
- handle.write calls useTerminalSession.write

Managed agent injection:

- managedAgentsStore resolves chat session to leaf id
- send_to_agent calls writeToSession(leafId, instruction)
- delayed carriage return submits input

Direct session injection:

- writeToSession(leafId, data) works without a live renderer handle
- if PTY is still opening, data is queued

Backend:

- pty_write already performs direct PTY writer.write_all by PTY id

### Option A: frontend-only terminal directory

Place target resolution in terminal domain above writeToSession.

Conceptual responsibilities:

- register/unregister leaves
- associate leaf with space, tab, pane, labels, agent kind, PTY id
- resolve name to leaf id
- call writeToSession

Advantages:

- smallest change
- reuses current authoritative topology
- no new dependency or process
- supports internal UI/AI sending immediately
- preserves lightweight design

Limits:

- unavailable before frontend is ready
- external clients cannot access it directly
- current boolean delivery semantics remain weak unless changed

Best use: first internal send_to_terminal milestone.

### Option B: Rust terminal registry

Add a backend registry mapping stable public terminal identity/name to PTY id. Frontend synchronizes lifecycle and metadata.

Advantages:

- direct writer access
- natural foundation for local API or named pipe
- independent of renderer handles

Limits:

- frontend owns semantic topology, so synchronization is required
- stale registration races must be handled
- naming policy crosses IPC
- duplicate frontend/backend authority risk

Best use: after identity contract stabilizes, before external control.

### Option C: expose direct command by PTY id

Add/send command accepting PTY id and bytes.

Advantages:

- minimal backend work
- lowest latency

Problems:

- PTY ids are backend-ephemeral implementation details
- callers still need name/leaf/PTy mapping
- encourages target logic in multiple modules
- weak external security boundary

Recommendation: do not make PTY id the public terminal address.

### Recommended staged hybrid

1. Canonical runtime identity remains leaf id.
2. Frontend TerminalDirectory owns semantic metadata and name resolution.
3. writeToSession remains initial transport.
4. PtyState gains reusable internal write method.
5. Later backend registry receives explicit register/update/unregister messages and maps public target id to PTY id.
6. Named pipe/local API calls the same internal writer method.

Recommended module boundaries, proposals only:

- frontend directory: src/modules/terminal/lib
- frontend identity types: src/modules/terminal/lib or tabs type boundary
- backend reusable writer: src-tauri/src/modules/pty
- external transport: new isolated backend module, dependent on terminal registry interface

Avoid:

- implementing target resolution in App.tsx
- implementing generic terminal routing inside AI modules
- treating customTitle as unique address without migration
- calling a Tauri command from Rust internal API code

### Target semantics requiring product decision

Before implementation:

- Is name unique app-wide, per space, or per tab?
- Can every split leaf be named independently?
- Does send inject raw text, bracketed paste, or text plus Enter?
- Is sender allowed to target private terminals?
- What happens when target is spawning, exited, closing, or ambiguous?
- Is delivery best-effort or must writer completion be returned?
- How does Agent A initiate send: UI tool, injected CLI command, local API, or named pipe client?

No code conclusively answers these.

## 7. Architecture capability evaluation

Effort scale:

- XS: less than 1 engineer-day
- S: 1 to 3 days
- M: 4 to 8 days
- L: 2 to 4 weeks
- XL: more than 1 month

Estimates include focused tests and documentation, not product polish across every UI.

| Capability | Current support | Complexity | Estimate | Main reason |
| --- | --- | --- | --- | --- |
| Named terminal tab labels | Already present via customTitle and persistence | XS | less than 1 day | Mostly exposed behavior and docs. |
| Unique addressable terminals | Partial | S-M | 2 to 6 days | Must define scope, pane naming, conflicts, lifecycle. |
| Terminal registry | Partial registries exist | M | 4 to 8 days | Must unify topology, runtime session, PTY mapping. |
| Direct internal PTY writes | Already present | XS-S | less than 2 days | writeToSession and pty_write exist; contract/tests missing. |
| Split panes | Already implemented | Existing | 0 for baseline | Pane tree, resizing, focus, close, swap exist. |
| External local API | Absent | L | 2 to 4 weeks | Auth, lifecycle, target registry, protocol, attack surface. |
| Windows named pipe server | Absent | M-L | 1 to 3 weeks | Windows API, ACL, framing, cancellation, shutdown, testing. |
| Plugin system | Absent | XL | 1 to 3 months minimum | ABI/API, permissions, isolation, versioning, lifecycle. |

### Named terminals

Easy for display labels. Moderate for addressable names.

Existing persistence already round-trips customTitle. Missing:

- uniqueness
- per-pane names
- stable public id
- target lookup
- rename event propagation to backend

### Terminal registry

Moderate. Existing data is sufficient but split:

- tabs/pane tree
- frontend sessions
- App refs
- managedAgentsStore
- PtyState

Main work is ownership, not storage.

### Direct PTY writes

Easy internally. Existing paths prove feasibility:

- active terminal write
- arbitrary leaf write
- managed agent follow-up write
- raw Rust PTY writer

Main work is reliable resolution and result semantics.

### Split panes

Already supported. Future terminal addressing must treat leaf as terminal, not tab.

### External local API

Technically feasible but materially expands security surface. Required:

- local-only bind
- authentication or unguessable capability
- request size limits
- target authorization
- private-terminal policy
- lifecycle and shutdown
- protocol versioning
- audit-safe logging without message content

### Named pipe server

Natural Windows-first transport. Prefer isolated Rust module using shared terminal registry interface. Do not embed pipe logic in pty/mod.rs.

Open design points:

- per-user ACL
- pipe name discovery
- single-instance behavior
- message framing
- client impersonation policy
- reconnect behavior
- request/response only, no queue

### Plugin system

High cost and not needed for first collaboration path. A plugin system would force decisions about trust, capabilities, API compatibility, UI integration, and native code. Defer until terminal-control API is stable enough to serve as a narrow extension surface.

## 8. Consolidated diagrams

Required diagrams appear in relevant sections:

1. Application architecture
2. PTY lifecycle
3. Terminal lifecycle
4. Tauri communication
5. State flow

## 9. Technical roadmap

### Phase 1: repository understanding

Status: completed by this report.

Objectives:

- establish code-backed architecture model
- correct tab-to-PTY cardinality
- inventory commands, state, processes, and extension points

Impacted modules:

- documentation only

Risks:

- architecture changes after baseline commit
- pre-existing docs drift

Effort:

- S

Exit criteria:

- this report reviewed and accepted

### Phase 2: terminal identity contract

Objectives:

- define terminal, tab, pane leaf, renderer slot, and PTY identities
- define stable public target id
- define addressable name scope and uniqueness
- define private-terminal and split-pane naming rules
- define best-effort delivery result

Impacted modules:

- src/modules/tabs/lib/useTabs.ts
- src/modules/terminal/lib/panes.ts
- src/modules/terminal/lib/useTerminalSession.ts
- src/modules/spaces/lib/serialize.ts
- docs/architecture

Risks:

- conflating display label with address
- breaking persisted space schema
- choosing tab identity when pane identity is required

Effort:

- S-M, 2 to 5 days

Exit criteria:

- reviewed ADR/spec
- no implementation required yet

### Phase 3: frontend terminal directory

Objectives:

- centralize live terminal metadata
- register/unregister leaves deterministically
- expose lookup by public id, name, tab id, and leaf id
- reuse writeToSession for delivery
- remove need for new routing code in App or AI modules

Impacted modules:

- src/modules/terminal/lib
- src/app/App.tsx only for topology publication/composition
- src/modules/tabs
- src/modules/agents/store/managedAgentsStore.ts

Risks:

- stale entries during pane close/workspace reset
- duplicate names
- React Strict Mode mount/unmount behavior
- hidden/spawning session behavior

Effort:

- M, 4 to 8 days

Exit criteria:

- registry invariant tests
- split-pane lifecycle tests
- no external transport

### Phase 4: named terminals

Objectives:

- expose addressable naming UI
- persist names with schema version/migration
- support pane-level identity when a tab is split
- show conflict and invalid-name feedback

Impacted modules:

- src/modules/tabs/TabBar.tsx
- src/modules/tabs/lib/tabLabel.ts
- src/modules/tabs/lib/useTabs.ts
- src/modules/spaces/lib/serialize.ts
- src/modules/terminal/lib

Risks:

- customTitle compatibility
- restore conflicts
- rename while external target lookup is active

Effort:

- S-M, 3 to 6 days

Exit criteria:

- labels remain presentation-safe
- addresses are unique in chosen scope
- round-trip persistence tests

### Phase 5: direct terminal injection

Objectives:

- add send_to_terminal(target, text, submitMode)
- return explicit result: resolved, queued locally, or written
- validate target state and payload bounds
- reuse existing raw PTY write path
- keep no mailbox, queue, broker, or persistence

Impacted modules:

- src/modules/terminal/lib
- src/modules/ai/tools only if exposed as an AI tool
- src-tauri/src/modules/pty/mod.rs for reusable writer method
- tests around pty_write and session lifecycle

Risks:

- control-character injection
- accidental Enter submission
- writes to private/wrong terminal
- close/write races
- caller assuming target application consumed bytes

Effort:

- M, 4 to 8 days

Exit criteria:

- direct leaf and name targeting
- split-pane coverage
- spawning/exited/closing behavior defined
- no queue or persistence introduced

### Phase 6: agent collaboration semantics

Objectives:

- identify Codex, Claude Code, Gemini CLI, and generic agent terminals
- allow Agent A context to resolve Agent B target
- define message formatting per CLI
- support reply injection using the same send path
- keep activity detection separate from message transport

Impacted modules:

- src/modules/agents
- src/modules/ai/tools/agent.ts
- src/modules/terminal/lib
- src-tauri/src/modules/pty/agent_detect.rs
- src-tauri/src/modules/agent.rs

Risks:

- agent-specific TUI readiness
- bracketed-paste differences
- prompts consumed by wrong TUI state
- recursive agent loops
- confusing status OSC with message acknowledgment

Effort:

- L, 2 to 4 weeks

Exit criteria:

- two different supported CLIs exchange messages through PTY writes
- no broker/mailbox/persistence
- loop and private-terminal policies tested

### Phase 7: backend registry and local control API

Objectives:

- mirror stable terminal target to PTY mapping in Rust
- expose one internal writer interface
- add authenticated local request/response API
- preserve direct injection semantics

Impacted modules:

- src-tauri/src/modules/pty
- new isolated control module
- src-tauri/src/lib.rs
- frontend terminal directory synchronization
- Tauri capabilities/security docs

Risks:

- stale mappings
- local privilege boundary
- unauthorized terminal injection
- multiple app instances
- shutdown races

Effort:

- L, 2 to 4 weeks

Exit criteria:

- local client can list allowed targets and inject bounded text
- per-user security
- no remote bind
- no queue

### Phase 8: Windows named pipe transport

Objectives:

- add Windows-first named pipe adapter over control API
- enforce per-user access
- add framed request/response protocol
- provide tiny CLI client if required

Impacted modules:

- new src-tauri control/windows module
- src-tauri/Cargo.toml or windows-sys feature set if required
- packaging and integration tests

Risks:

- ACL mistakes
- partial frames
- abandoned clients
- app restart/discovery
- named pipe squatting

Effort:

- M-L, 1 to 3 weeks

Exit criteria:

- same-user client can inject into named terminal
- unauthorized user rejected
- clean shutdown and cancellation

### Phase 9: plugin boundary, optional

Objectives:

- decide whether stable control API is sufficient
- only if insufficient, define capability-based plugin API
- separate trusted native plugins from sandboxed declarative extensions

Impacted modules:

- whole application boundary
- packaging
- security model
- versioning policy

Risks:

- largest security and maintenance expansion
- bundle and startup cost
- compatibility burden
- native crash surface

Effort:

- XL, 1 to 3 months minimum

Exit criteria:

- explicit use cases not served by control API
- permission model and compatibility policy approved

## Open questions and required investigation

1. Name scope: application, space, tab, or pane.
2. Whether customTitle should migrate, coexist, or remain presentation-only.
3. Exact send contract: raw text, paste, bracketed paste, Enter, or configurable.
4. Whether private terminals reject all programmatic injection or only reads.
5. Agent-origin mechanism for Agent A: AI tool, terminal CLI, named pipe, or local API.
6. Reply definition: explicit agent command versus inferred terminal output.
7. portable-pty 0.9 ConPTY behavior under concurrent create/close, if low-level changes are considered.
8. Actual webview OS process topology per Windows WebView2 version, only if process-level resource budgets require it.

## Final architectural recommendation

Do not start with a new Tauri command named send_to_terminal.

Start with identity and ownership:

- public target identity
- leaf-based runtime directory
- lifecycle registration
- explicit delivery result

Then reuse existing transport:

- writeToSession for frontend-local routing
- PtyState writer method for backend/external routing

This minimizes technical debt, respects Tauri/Rust boundaries, preserves low latency, and avoids introducing any mailbox, broker, queue, persistence, or orchestration layer.
