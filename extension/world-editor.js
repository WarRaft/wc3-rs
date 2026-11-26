'use strict'

/**
 * The World Editor (see the wc3-rs README, "World Editor"): a page per
 * project (`wc3.bni`) showing its map, and behind it a process of its own,
 * `wc3 editor --project <root>` (`../editor`), that does the work. The two
 * speak the editor protocol (`editor/src/protocol.rs`, its twin
 * `webview/editor/protocol.js`); this file only passes their frames
 * between them, as bytes, without reading them, and keeps one editor per
 * project: opening it again shows the one open. The process holds a lock
 * of the project too, so another VS Code window can't open a second one
 * (it ends at once with `EXIT_LOCKED`, and the page says so).
 *
 * The view WC3 Editors lists the workspace's projects and which have their
 * editor open; the command `wc3.editor.open` opens one (from the view, the
 * button over a `wc3.bni`, the Explorer's menu, the palette).
 */

const path = require('path')
const {spawn} = require('child_process')
const {window, workspace, commands, Uri, ViewColumn, EventEmitter, TreeItem, TreeItemCollapsibleState, ThemeIcon, ThemeColor} = require('vscode')
const {FrameReader} = require('./protocol.js')

/** `wc3_editor::EXIT_LOCKED`: another window has the project's editor. */
const EXIT_LOCKED = 3
const VIEW = 'wc3.editors'

/**
 * @typedef {object} Editor
 * @property {string} root  the project's folder
 * @property {import('vscode').WebviewPanel} panel
 * @property {import('child_process').ChildProcess | null} child
 * @property {{name: string, width: number, height: number} | null} map  what the page last said of the map
 * @property {string | null} error  what the page last said went wrong
 * @property {{pathing: number, regions: number, cameras: number, bounds: number, minimap: number, region?: {name: string, x: number, y: number}} | undefined} overlays  what the page has of the overlays
 * @property {{dirty: boolean, undo: boolean, redo: boolean, message: string} | undefined} state  the edits' state the page last showed
 * @property {string | undefined} picked  what the last click picked, said ('' nothing)
 * @property {{width: number, height: number, layers: number, water: number, cliffs: number, objects: number, error: number} | null} drawn  what the page drew of the terrain first
 */

/**
 * The bytes of a frame the page sent: a typed array or `ArrayBuffer` as
 * VS Code passes them, or what one turns into where it serializes them
 * (an array, an object of indices).
 */
function bytesOf(data) {
    if (data instanceof Uint8Array) return Buffer.from(data.buffer, data.byteOffset, data.byteLength)
    if (data instanceof ArrayBuffer) return Buffer.from(data)
    if (Array.isArray(data)) return Buffer.from(data)
    return Buffer.from(Object.keys(data ?? {}).sort((a, b) => a - b).map(k => data[k]))
}

/**
 * @param {import('vscode').ExtensionContext} context
 * @param {string} binary  the `wc3` binary
 * @param {(arg: unknown, title: string) => Promise<string | null>} pickManifest  the `wc3.bni` a command is about
 * @returns {Map<string, Editor>} the editors open, by project folder (for the tests)
 */
function registerWorldEditor(context, binary, pickManifest) {
    const media = Uri.joinPath(context.extensionUri, 'webview', 'editor')
    /** @type {Map<string, Editor>} */
    const editors = new Map()
    const listChanged = new EventEmitter()
    const refresh = () => listChanged.fire(undefined)

    const html = webview => {
        const nonce = [...Array(32)].map(() => Math.floor(Math.random() * 36).toString(36)).join('')
        const asset = name => webview.asWebviewUri(Uri.joinPath(media, name))
        return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}'">
<link rel="stylesheet" href="${asset('editor.css')}">
</head><body>
<div class="bar"><span id="name" class="name">Opening…</span><span id="meta" class="meta"></span><span class="spacer"></span><span id="toggles" class="toggles"></span><span class="hint">Click: pick (drag it to move, Q/E turn, Delete) · Right drag: move · Ctrl + right drag: turn · Wheel: zoom · WASD · G: grid · Home: all of it</span></div>
<div class="stage"><canvas id="view"></canvas><canvas id="minimap" class="minimap" hidden></canvas>
<div id="tools" class="tools">
  <div id="tool-buttons" class="tool-buttons"></div>
  <label id="size-label">Size <span id="size-value"></span><input id="size" type="range" min="1" max="32" step="1"></label>
  <label id="strength-label">Strength <span id="strength-value"></span><input id="strength" type="range" min="0.05" max="1" step="0.05"></label>
  <div id="textures" class="textures" hidden></div>
  <select id="cliff-types" hidden></select>
  <div id="objects-palette" class="objects-palette" hidden>
    <div id="object-kinds" class="object-kinds"></div>
    <input id="object-search" type="search" placeholder="Search">
    <select id="object-player"></select>
    <div id="object-list" class="object-list"></div>
  </div>
  <div class="edits"><button id="undo" title="Undo (Ctrl+Z)" disabled>Undo</button><button id="redo" title="Redo (Ctrl+Shift+Z)" disabled>Redo</button><button id="save" title="Save the terrain (Ctrl+S)" disabled>Save</button></div>
</div>
<div id="message" class="message" hidden><h2 id="message-title"></h2><p id="message-text"></p><button id="reopen" hidden>Reopen</button></div>
</div>
<ul id="notes" class="notes" hidden></ul>
<div id="status" class="status"></div>
<script nonce="${nonce}" src="${asset('protocol.js')}"></script>
<script nonce="${nonce}" src="${asset('camera.js')}"></script>
<script nonce="${nonce}" src="${asset('terrain.js')}"></script>
<script nonce="${nonce}" src="${asset('objects.js')}"></script>
<script nonce="${nonce}" src="${asset('overlays.js')}"></script>
<script nonce="${nonce}" src="${asset('editor.js')}"></script>
</body></html>`
    }

    /** Starts the editor's process, its frames passed to the page. */
    const start = editor => {
        const child = spawn(binary, ['editor', '--project', editor.root], {cwd: editor.root, stdio: ['pipe', 'pipe', 'pipe']})
        editor.child = child
        let stderr = ''
        const frames = new FrameReader()
        child.stdout.on('data', chunk => {
            for (const {kind, body} of frames.push(chunk)) {
                editor.panel.webview.postMessage({type: 'frame', kind, body: new Uint8Array(body)})
            }
        })
        child.stderr.on('data', d => {
            stderr = (stderr + d).slice(-4000)
            console.error(`[wc3 editor] ${d.toString().trimEnd()}`)
        })
        const ended = (code, message) => {
            if (editor.child !== child) return
            editor.child = null
            refresh()
            // Closed by us: nothing to say.
            if (!editors.has(editor.root)) return
            const locked = code === EXIT_LOCKED
            editor.panel.webview.postMessage({
                type: 'exit',
                locked,
                message: message || stderr.trim().replace(/^wc3 editor: /gm, '') || `The editor's process ended (${code}).`,
            })
        }
        child.on('error', e => ended(-1, `Can't start ${binary}: ${e.message}`))
        child.on('exit', code => ended(code))
        refresh()
    }

    const stop = editor => {
        const child = editor.child
        editor.child = null
        if (!child) return
        // Closing stdin is its signal to end; then, if it doesn't, a kill.
        child.stdin.end()
        setTimeout(() => { if (child.exitCode === null) child.kill() }, 3000)
    }

    /** Opens the editor of the project of `arg`, or shows it if it's open. */
    const open = async arg => {
        const manifest = arg?.root ? path.join(arg.root, 'wc3.bni') : await pickManifest(arg, 'Open the World Editor of which project?')
        if (!manifest) {
            window.showWarningMessage('WC3: no wc3.bni in the workspace or above the active file.')
            return null
        }
        const root = path.dirname(manifest)
        const open = editors.get(root)
        if (open) {
            open.panel.reveal()
            return open
        }
        const panel = window.createWebviewPanel('wc3.worldEditor', path.basename(root), ViewColumn.Active, {
            enableScripts: true,
            retainContextWhenHidden: true,
            localResourceRoots: [media],
        })
        panel.iconPath = new ThemeIcon('globe')
        /** @type {Editor} */
        const editor = {root, panel, child: null, map: null, error: null, drawn: null}
        editors.set(root, editor)
        panel.webview.html = html(panel.webview)
        panel.webview.onDidReceiveMessage(msg => {
            if (msg.type === 'frame') {
                editor.child?.stdin.write(bytesOf(msg.data))
            } else if (msg.type === 'map') {
                editor.map = msg.map
                editor.error = null
                panel.title = (editor.state?.dirty ? '● ' : '') + (msg.map.name || path.basename(root))
                refresh()
            } else if (msg.type === 'state') {
                editor.state = msg.state
                const name = editor.map?.name || path.basename(root)
                panel.title = msg.state.dirty ? `● ${name}` : name
            } else if (msg.type === 'drawn') {
                editor.drawn = msg.terrain
            } else if (msg.type === 'picked') {
                editor.picked = msg.text
            } else if (msg.type === 'overlays') {
                editor.overlays = msg.overlays
            } else if (msg.type === 'error') {
                editor.error = msg.message
            } else if (msg.type === 'reopen') {
                stop(editor)
                start(editor)
                panel.webview.postMessage({type: 'restarted'})
            }
        })
        panel.onDidDispose(() => {
            if (editor.state?.dirty) window.showWarningMessage(`WC3: the World Editor of ${path.basename(root)} was closed with changes not saved; they're lost.`)
            editors.delete(root)
            stop(editor)
            refresh()
        })
        start(editor)
        return editor
    }

    const close = node => {
        const root = node?.root ?? (node instanceof Uri ? path.dirname(node.fsPath) : null)
        editors.get(root)?.panel.dispose()
    }

    // The projects of the workspace; a click opens one's editor.
    const byName = (a, b) => a.localeCompare(b, undefined, {sensitivity: 'base'})
    const list = {
        onDidChangeTreeData: listChanged.event,
        async getChildren(node) {
            if (node) return []
            const found = await workspace.findFiles('**/wc3.bni', '**/node_modules/**')
            return found
                .map(uri => path.dirname(uri.fsPath))
                .sort((a, b) => byName(path.basename(a), path.basename(b)) || byName(a, b))
                .map(root => ({root}))
        },
        getTreeItem({root}) {
            const editor = editors.get(root)
            const item = new TreeItem(path.basename(root), TreeItemCollapsibleState.None)
            item.id = root
            item.iconPath = editor ? new ThemeIcon('globe', new ThemeColor('charts.green')) : new ThemeIcon('globe')
            const where = workspace.asRelativePath(root)
            item.description = [editor?.map?.name, where !== root && where !== path.basename(root) ? where : ''].filter(Boolean).join(' · ')
            item.tooltip = `${root}${editor ? '\nThe World Editor is open.' : ''}`
            item.contextValue = editor ? 'wc3.project.open' : 'wc3.project'
            item.command = {command: 'wc3.editor.open', title: 'Open World Editor', arguments: [{root}]}
            return item
        },
    }

    const watcher = workspace.createFileSystemWatcher('**/wc3.bni', false, true, false)
    context.subscriptions.push(
        listChanged,
        watcher,
        watcher.onDidCreate(refresh),
        watcher.onDidDelete(refresh),
        workspace.onDidChangeWorkspaceFolders(refresh),
        window.createTreeView(VIEW, {treeDataProvider: list}),
        commands.registerCommand('wc3.editor.open', arg => open(arg).catch(e => window.showErrorMessage(`WC3: ${e.message}`))),
        commands.registerCommand('wc3.editor.close', close),
        commands.registerCommand('wc3.editors.refresh', refresh),
        {dispose: () => editors.forEach(e => e.panel.dispose())},
    )
    return editors
}

module.exports = {registerWorldEditor}
