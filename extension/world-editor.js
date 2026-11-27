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
const {window, workspace, commands, Uri, ViewColumn, EventEmitter, TreeItem, TreeItemCollapsibleState, ThemeIcon, ThemeColor, WorkspaceEdit, Range} = require('vscode')
const {FrameReader} = require('./protocol.js')

/**
 * The editor protocol's kinds this file reads or writes itself (the rest
 * pass between the process and the page as they are): the text forms of
 * the map's pieces, kept in step with the editor (`editor/src/protocol.rs`).
 */
const KIND = {TEXT: 17, TEXT_CLOSED: 18, IMPORT_ADD: 41, FOLLOW: 142, TEXT_OUT: 143, SAVE_DOCS: 144}
/** The scheme text forms of binary files open under (`extension.js`). */
const OBJECTS_SCHEME = 'wc3obj'

/** A frame of `kind` with strings. */
function frame(kind, ...strings) {
    const parts = strings.map(s => Buffer.from(s, 'utf8'))
    const body = Buffer.alloc(parts.reduce((n, p) => n + 4 + p.length, 0))
    let at = 0
    for (const p of parts) {
        body.writeUInt32LE(p.length, at)
        p.copy(body, at + 4)
        at += 4 + p.length
    }
    const head = Buffer.alloc(5)
    head.writeUInt32LE(body.length, 0)
    head[4] = kind
    return Buffer.concat([head, body])
}

/** The strings of a body, `n` of them, or a list of them (`list`). */
function strings(body, n, list = false) {
    let at = 0
    const one = () => {
        const len = body.readUInt32LE(at)
        const s = body.toString('utf8', at + 4, at + 4 + len)
        at += 4 + len
        return s
    }
    if (!list) return Array.from({length: n}, one)
    const count = body.readUInt32LE(0)
    at = 4
    return Array.from({length: count}, () => Array.from({length: n}, one))
}

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
 * @property {{pathing: number, destructables: number, decks: number, regions: number, cameras: number, bounds: number, minimap: number, region?: {name: string, x: number, y: number}} | undefined} overlays  what the page has of the overlays
 * @property {{dirty: boolean, undo: boolean, redo: boolean, message: string} | undefined} state  the edits' state the page last showed
 * @property {{name: string, bounds: number[]} | null | undefined} region  the region picked (the Regions tool)
 * @property {{index: number, name: string, target: number[], view: number[], count: number} | null | undefined} camera  the camera picked (the Cameras tool; `view` rotation, angle of attack, distance, field of view), and how many there are
 * @property {{frames: number, blocks: number, blinks: number, where: object[]} | undefined} flicker  blinks the page counted over a run of frames
 * @property {string | false | undefined} focused  the model of the thing the camera was put over (one with ribbons, or else tails)
 * @property {{width: number, height: number, margins: number[], tileset: string, mapTiles: string[], tiles: number} | undefined} mapSettings  the map's settings the page was last sent
 * @property {{day: boolean, shadows: number, fog: boolean} | undefined} light  the map's light the page has
 * @property {number | undefined} icons  how many of the object palette's icons the page has
 * @property {Map<string, string> | undefined} follow  the paths of the pieces with a text form, by their lower case
 * @property {string | undefined} picked  what the last click picked, said ('' nothing)
 * @property {{kind: number, count: number, custom: string[], changed: string[], picked: {id: string, base: string, name: string, levels: number, fields: [string, number, string, boolean][]} | null} | undefined} objects  the object editor's kind, its objects, the one picked and its fields
 * @property {Object<string, string> | undefined} info  the map's info as its panel last showed it (`Session::info_fields`)
 * @property {{files: [string, boolean, boolean][], picked: number | null, preview: {path: string, what: number, size: number, width: number, error: string} | null} | undefined} imports  the import manager's list (path, found, to be written), the one picked and what it is
 * @property {{count: number, names: string[], picked: object | null} | undefined} sounds  the map's sounds and the one picked (the Sounds tool)
 * @property {{file: string, bytes: number, error: string, played: boolean} | undefined} played  the last sound file played: its bytes, whether it played
 * @property {{file: number, index: number, fields: [string, number][]} | null | undefined} thing  the thing picked and its fields as its panel shows them (the Select tool)
 * @property {{width: number, height: number, layers: number, water: number, cliffs: number, objects: number, animated: number, billboards: number, startLocations: number, geosetAnimated: number, emitters: number, tails: number, modelSpace: number, ribbons: number, error: number} | null} drawn  what the page drew of the terrain first
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
<div class="bar"><span id="name" class="name">Opening…</span><span id="meta" class="meta"></span><span class="spacer"></span><label class="hours" title="The time of day">Time <input id="hours" type="range" min="0" max="24" step="0.25"> <span id="hours-value"></span></label><span id="toggles" class="toggles"></span><span class="hint">Click: pick (drag it to move, Q/E turn, Delete) · Right drag: move · Ctrl + right drag: turn · Wheel: zoom · WASD · G: grid · Home: all of it</span></div>
<div class="stage"><canvas id="view"></canvas><canvas id="minimap" class="minimap" hidden></canvas>
<div id="tools" class="tools">
  <div id="tool-buttons" class="tool-buttons"></div>
  <label id="size-label">Size <span id="size-value"></span><input id="size" type="range" min="1" max="32" step="1"></label>
  <label id="strength-label">Strength <span id="strength-value"></span><input id="strength" type="range" min="0.05" max="1" step="0.05"></label>
  <div id="textures" class="textures" hidden></div>
  <select id="cliff-types" hidden></select>
  <div id="pathing-flags" class="pathing-flags" hidden>
    <label><input type="checkbox" value="2"> No walking</label>
    <label><input type="checkbox" value="4"> No flying</label>
    <label><input type="checkbox" value="8"> No building</label>
  </div>
  <div id="region-panel" class="region-panel" hidden>
    <input id="region-name" type="text" placeholder="Region's name" disabled>
    <span id="region-bounds" class="region-bounds"></span>
    <button id="region-delete" disabled>Delete region</button>
  </div>
  <div id="camera-panel" class="region-panel" hidden>
    <select id="camera-list" class="camera-list" size="6"></select>
    <input id="camera-name" type="text" placeholder="Camera's name" disabled>
    <span id="camera-hint" class="region-bounds"></span>
    <div id="camera-fields" class="camera-fields"></div>
    <button id="camera-new" title="A new camera, looking as the view does">New from view</button>
    <button id="camera-set" disabled title="The camera picked looks as the view does">Set to view</button>
    <button id="camera-look" disabled title="The view looks as the camera picked does">Look through it</button>
    <button id="camera-delete" disabled>Delete camera</button>
  </div>
  <div id="sound-panel" class="region-panel" hidden>
    <select id="sound-list" class="camera-list" size="8"></select>
    <input id="sound-new-file" type="text" placeholder="Sound\\Ambient\\…\\….wav">
    <button id="sound-browse" title="The game's and the map's sound files, to pick from">Browse…</button>
    <div id="sound-browser" class="sound-browser" hidden>
      <input id="sound-browse-search" type="search" placeholder="Search the sound files">
      <div id="sound-browse-list" class="obj-list"></div>
    </div>
    <button id="sound-new" title="A new sound of that file">New sound</button>
    <span id="sound-hint" class="region-bounds"></span>
    <div id="sound-fields" class="camera-fields"></div>
    <button id="sound-play" disabled title="Plays its file">Play</button>
    <button id="sound-delete" disabled>Delete sound</button>
  </div>
  <div id="thing-panel" class="region-panel" hidden>
    <span id="thing-hint" class="region-bounds"></span>
    <div id="thing-fields" class="camera-fields"></div>
    <div id="thing-lists" class="thing-lists"></div>
  </div>
  <div id="objects-palette" class="objects-palette" hidden>
    <div id="object-kinds" class="object-kinds"></div>
    <input id="object-search" type="search" placeholder="Search">
    <select id="object-player"></select>
    <div id="object-list" class="object-list"></div>
  </div>
  <div class="edits"><button id="undo" title="Undo (Ctrl+Z)" disabled>Undo</button><button id="redo" title="Redo (Ctrl+Shift+Z)" disabled>Redo</button><button id="save" title="Save the files changed (Ctrl+S)" disabled>Save</button><button id="reload" title="Read the map again, the changes not saved dropped">Reload</button></div>
  <button id="map-open" title="The map's size, its tiles and their pathing">Map…</button>
  <button id="info-open" title="The map's info: its description, loading screen, options, environment, players and forces">Info…</button>
  <button id="imports-open" title="The import manager: the map's imported files">Imports…</button>
  <button id="objects-open" title="The object editor: units, items, destructables, doodads, abilities, buffs, upgrades">Objects…</button>
</div>
<div id="map-panel" class="map-panel" hidden></div>
<div id="objects-panel" class="obj-panel" hidden></div>
<div id="imports-panel" class="obj-panel" hidden></div>
<div id="info-panel" class="obj-panel info-panel" hidden></div>
<div id="message" class="message" hidden><h2 id="message-title"></h2><p id="message-text"></p><button id="reopen" hidden>Reopen</button></div>
</div>
<ul id="notes" class="notes" hidden></ul>
<div id="status" class="status"></div>
<script nonce="${nonce}" src="${asset('protocol.js')}"></script>
<script nonce="${nonce}" src="${asset('camera.js')}"></script>
<script nonce="${nonce}" src="${asset('light.js')}"></script>
<script nonce="${nonce}" src="${asset('mapsettings.js')}"></script>
<script nonce="${nonce}" src="${asset('objecteditor.js')}"></script>
<script nonce="${nonce}" src="${asset('thinglists.js')}"></script>
<script nonce="${nonce}" src="${asset('imports.js')}"></script>
<script nonce="${nonce}" src="${asset('mapinfo.js')}"></script>
<script nonce="${nonce}" src="${asset('terrain.js')}"></script>
<script nonce="${nonce}" src="${asset('particles.js')}"></script>
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
                if (kind === KIND.FOLLOW) follow(editor, strings(body, 2, true))
                else if (kind === KIND.TEXT_OUT) textOut(...strings(body, 2))
                else if (kind === KIND.SAVE_DOCS) saveDocs(strings(body, 1, true).map(([p]) => p))
                else editor.panel.webview.postMessage({type: 'frame', kind, body: new Uint8Array(body)})
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

    // The text forms of the map's pieces (war3map.doo ... open as text):
    // what's typed in them goes to the editor, what the editor changes
    // goes into them (they have it unsaved then), saving them is theirs.
    const key = p => p.toLowerCase()
    // A text form (war3map.doo as text ...), or the map's strings, text
    // themselves (on disk, or inside the map).
    const followed = d => d.uri.scheme === OBJECTS_SCHEME || /\.wts$/i.test(d.uri.path)
    const docOf = path => workspace.textDocuments.find(d => followed(d) && key(d.uri.fsPath) === key(path))
    const editorOf = doc => followed(doc) && [...editors.values()].find(e => e.follow?.has(key(doc.uri.fsPath)))
    const sendText = doc => {
        const e = editorOf(doc)
        if (e?.child) e.child.stdin.write(frame(KIND.TEXT, doc.uri.fsPath, doc.getText()))
    }
    function follow(editor, pieces) {
        editor.follow = new Map(pieces.map(([, p]) => [key(p), p]))
        for (const d of workspace.textDocuments) if (editorOf(d) === editor) sendText(d)
    }
    async function textOut(path, text) {
        const doc = docOf(path)
        if (!doc || doc.getText() === text) return
        const edit = new WorkspaceEdit()
        edit.replace(doc.uri, new Range(doc.positionAt(0), doc.positionAt(doc.getText().length)), text)
        await workspace.applyEdit(edit)
    }
    function saveDocs(paths) {
        for (const p of paths) docOf(p)?.save()
    }
    const timers = new Map()
    context.subscriptions.push(
        workspace.onDidOpenTextDocument(sendText),
        workspace.onDidChangeTextDocument(e => {
            if (!editorOf(e.document)) return
            const k = e.document.uri.toString()
            clearTimeout(timers.get(k))
            timers.set(k, setTimeout(() => sendText(e.document), 150))
        }),
        workspace.onDidCloseTextDocument(doc => {
            const e = editorOf(doc)
            if (e?.child) e.child.stdin.write(frame(KIND.TEXT_CLOSED, doc.uri.fsPath))
        }),
    )

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
            } else if (msg.type === 'flicker') {
                editor.flicker = msg.flicker
            } else if (msg.type === 'focused') {
                editor.focused = msg.found && msg.path
            } else if (msg.type === 'mapSettings') {
                editor.mapSettings = msg.settings
            } else if (msg.type === 'light') {
                editor.light = msg.light
            } else if (msg.type === 'icons') {
                editor.icons = msg.icons
            } else if (msg.type === 'region') {
                editor.region = msg.region
            } else if (msg.type === 'camera') {
                editor.camera = msg.camera
            } else if (msg.type === 'importPick') {
                // Files to import, from VS Code's dialog: each at war3mapImported\ and its name.
                window.showOpenDialog({canSelectMany: true, openLabel: 'Import', title: 'Files to import into the map'}).then(uris => {
                    for (const uri of uris ?? []) editor.child?.stdin.write(frame(KIND.IMPORT_ADD, uri.fsPath, ''))
                })
            } else if (msg.type === 'info') {
                editor.info = msg.info
            } else if (msg.type === 'imports') {
                editor.imports = msg.imports
            } else if (msg.type === 'objects') {
                editor.objects = msg.objects
            } else if (msg.type === 'soundFiles') {
                editor.soundFiles = msg.soundFiles
            } else if (msg.type === 'sounds') {
                editor.sounds = msg.sounds
            } else if (msg.type === 'played') {
                editor.played = msg.played
            } else if (msg.type === 'thing') {
                editor.thing = msg.thing
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
