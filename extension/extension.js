'use strict'

/**
 * WC3 extension entry point: a thin shell around the language backend,
 * `wc3 server` (see `../server`; `../cli` is the one binary, `wc3`, whose
 * `wc3 mcp` is the MCP server). All language logic is in Rust.
 *
 * The extension starts the server and talks to it over stdio in a small
 * binary protocol (`protocol.js`). It only reports what happens to
 * documents (opened, edited, language changed, closed) plus the workspace
 * folders. The server keeps every file of every project in memory, watches
 * the disk itself, and answers each change with one message per file
 * holding everything it knows about it (diagnostics, and semantic tokens
 * for open documents), including files that were never opened. Nothing
 * here asks the server anything: diagnostics are applied as they arrive,
 * and highlighting is served from the last tokens received. Only what
 * depends on the cursor is asked: go to definition, find references,
 * hover, symbol search and rename are answered from the server's symbol
 * index.
 *
 * Scripts inside maps (`.w3x`, `.w3m`) are files of the model too, at a
 * path inside the map's (`<map.w3x>/war3map.j`), and so are the game's
 * `common.j` and `Blizzard.j` when they come from its `.mpq`s. VS Code can't read them,
 * so they're shown under the `wc3map:` scheme with that path: a file
 * system whose every operation the server carries out with `storm-rs`
 * (`server/src/mapfs.rs`). A map opens as a workspace folder (Open Map as
 * Folder, from the Explorer, the WC3 Maps list, or the page a map file
 * opens in), and then its files are listed, opened, saved, copied, added,
 * renamed and deleted like any others. The game's `.mpq`s are read-only.
 *
 * A `.blp` texture opens in a viewer (`webview/texture.js`): the extension
 * reads the file (from disk or from inside a map), the server decodes it
 * with `blp-rs` (`server/src/texture.rs`), and the page draws its mips. A
 * `.tga` (map previews, loading screens) opens there too, decoded with the
 * `image` crate.
 * A shadow map (`.shd`) too, its size its terrain's (the `war3map.w3e`
 * beside it, which the server reads). A pathing map (`.wpm`), which has
 * no text, opens there too: the server
 * draws it as the game's pathing textures draw pathing (red: no walking,
 * green: no flying, blue: no building) and sends each cell's flags, and
 * the page shows one flag at a time and names a cell's flags under the
 * cursor.
 *
 * Object data (`.w3a`, `.w3b`, `.w3d`, `.w3h`, `.w3q`, `.w3t`, `.w3u`)
 * opens as text: its BNI text form (`w3abdhqtu-rs`), under the `wc3obj:`
 * scheme with the file's path. That file system reads the file (from disk
 * or from inside a map) and has the server turn it into text; saving has
 * the server turn the text back into the file's bytes, and fails with the
 * reason when it can't. While it's edited, the text is a BNI document like
 * any other (highlighting, diagnostics), under the language of its file
 * (`w3a` ...), which the server reads as BNI.
 *
 * Cameras (`.w3c`) open the same way, through the same scheme: their text
 * form is BNI too, a section per camera (`w3c-rs`, `server/src/cameras.rs`).
 * Which fields a camera has depends on the game (1.31 added a local
 * rotation), which the server reads from the map's `war3map.w3i`.
 * Regions (`.w3r`) too: a section per region (`w3r-rs`,
 * `server/src/regions.rs`), a weather's name as an inlay hint after its
 * id, and `color=` as a color the picker changes. Sounds (`.w3s`) too
 * (`w3s-rs`, `server/src/sounds.rs`), with the names of their EAX
 * setting, channel and flags as inlay hints. The custom script (`.wct`)
 * too (`wct-rs`, `server/src/customtext.rs`): its text is the code, one
 * piece after another after `// @wct` marker lines, and it's code of the
 * project in the server's model; while open, the text stands for the
 * file there.
 *
 * Terrain (`.w3e`) too, but only its header: tilesets, size, position
 * (`w3e-rs`, `server/src/terrain.rs`). Saving puts the header onto the
 * file as it is, its points kept, and fails when they don't allow it.
 *
 * txt2 (`.txt2`, XGM's markup) is highlighted by the server
 * (`txt2-rs`) and has a preview beside it, as Markdown does
 * (`registerTxt2Preview`).
 *
 * Map info (`.w3i`) too: a `[w3i]` section, then a section per player,
 * force, upgrade, tech and random table (`w3i-rs`, `server/src/mapinfo.rs`).
 * Imported files (`.imp`, `imp-rs`) and minimap icons (`.mmp`, `mmp-rs`,
 * colors in the picker) too. What a map places (`.doo`: `war3map.doo`
 * doodads, `war3mapUnits.doo` units, its own language) too (`doo-rs`,
 * `server/src/placement.rs`), the layout from the `war3map.w3i` beside it.
 * Triggers (`.wtg`) too (`wtg-rs`, `server/src/triggers.rs`): their
 * events, conditions and actions as lines of calls; the server reads them
 * with the project's `UI\TriggerData.txt`. A trigger of its own (`.trg`,
 * one `[trigger]` of that text, its folder its category) opens as World
 * Editor shows it (`registerTrigger`, `webview/triggers.js`), and so does the
 * `.wtg`'s text, with the map's tree beside (`webview/triggers.js` with `wtg`); Show JASS
 * shows what it's built into (`registerTriggerJass`). A collection of object data
 * (`.w3o`, World Editor's export) too: `[w3o]`, then each kind's text form.
 *
 * In `wc3.bni`, the paths of `game=`, `sources=`, `exclude=` and `root=`
 * complete from the disk and every folder along them is a link that reveals that folder (in the
 * Explorer when it's in the workspace, in the system's file manager
 * otherwise). The server finds both (`server/src/paths.rs`).
 */

const path = require('path')
const fs = require('fs')
const {spawn} = require('child_process')
const {
    workspace,
    languages,
    window,
    commands,
    CompletionItem,
    CompletionItemKind,
    DocumentLink,
    Uri,
    Range,
    Location,
    Hover,
    MarkdownString,
    SymbolInformation,
    SymbolKind,
    WorkspaceEdit,
    Diagnostic,
    EventEmitter,
    SemanticTokens,
    SemanticTokensLegend,
    ThemeColor,
    TreeItem,
    TreeItemCollapsibleState,
    FileSystemError,
    FileType,
    FilePermission,
    FileChangeType,
    FileDecoration,
    Disposable,
    RelativePattern,
    CodeAction,
    CodeActionKind,
    InlayHint,
    Position,
    Color,
    ColorInformation,
    ColorPresentation,
    ViewColumn,
    Selection,
    TextEditorRevealType,
    env,
    lm,
    McpStdioServerDefinition,
    CodeLens,
    ProgressLocation,
} = require('vscode')
const {encode, decode, FrameReader, QUERY, FS_OP, FS_FLAG, OBJECTS_OP} = require('./protocol.js')
const {registerWorldEditor} = require('./world-editor.js')
const {triggerMarkup, triggerHead} = require('./trigger-markup.js')

/** The scheme of files inside maps. */
const MAP_SCHEME = 'wc3map'

/** Archives a path can lead into: maps, and the game's `.mpq`s (the
 * server's `map::ARCHIVES`). */
const MAP_NAME = /\.(w3[xm]|mpq)$/i

/** The scheme of object data shown as text. */
const OBJECTS_SCHEME = 'wc3obj'
/** A trigger's JASS, read-only (`registerTriggerJass`). */
const JASS_SCHEME = 'wc3jass'
/**
 * What a trigger is shown built into (`registerTriggerJass`): each
 * language's scheme, file extension, query, command, name and icon.
 */
const SHOWN = [
    {id: 'jass', scheme: JASS_SCHEME, ext: 'j', query: 'TRIGGER_JASS', command: 'wc3.trg.showJass', name: 'JASS', icon: 'jass.svg'},
    {id: 'lua', scheme: 'wc3lua', ext: 'lua', query: 'TRIGGER_LUA', command: 'wc3.trg.showLua', name: 'Lua', icon: 'lua.svg'},
    {id: 'angelscript', scheme: 'wc3as', ext: 'as', query: 'TRIGGER_ANGELSCRIPT', command: 'wc3.trg.showAngelScript', name: 'AngelScript', icon: 'as.svg'},
]
/** A piece of a map's custom script, as a document of its own (`registerPieces`). */
const PIECE_SCHEME = 'wc3code'

/** The languages of object-data files (`package.json`), one per type, of
 * camera files (`w3c`), terrain (`w3e`), map info (`w3i`), regions
 * (`w3r`), sounds (`w3s`) and the custom script (`wct`, whose text is
 * code: JASS, or Lua in a Lua map). As `wc3obj:` text they're BNI; a file opened as is is binary,
 * not text. */
const OBJECT_LANGUAGES = new Set(['w3a', 'w3b', 'w3d', 'w3h', 'w3q', 'w3t', 'w3u', 'w3c', 'w3e', 'w3i', 'w3r', 'w3s', 'wct', 'imp', 'mmp', 'doo', 'wc3-units', 'wtg', 'w3o'])

/** Language ids the server analyzes (its `Language::from_id`). */
const LANGUAGES = new Set(['jass', 'vjass', 'zinc', 'lua', 'angelscript', 'bni', 'slk', 'wts', 'txt2', 'trg', ...OBJECT_LANGUAGES])

/** How long highlighting waits for the server to analyze a new version. */
const TOKENS_WAIT_MS = 1000

/** @type {import('child_process').ChildProcess | null} */
let server = null

/** @type {import('vscode').DiagnosticCollection} */
let diagnostics

/** Documents open in the editor that the server knows: id → languageId. */
const tracked = new Map()

/**
 * VS Code reports a language change as close + open of the same document
 * in one go. A close waits a tick, so an open right after it becomes one
 * `lang` message instead. id → timer.
 */
const pendingClose = new Map()

/** The last tokens per open document: id → {version, data}. */
const tokens = new Map()

/** Highlighting requests waiting for a newer version: id → [{version, done}]. */
const waiting = new Map()

const tokensChanged = new EventEmitter()

/** The quick fix the server sent with a diagnostic. */
const fixes = new WeakMap()

/** How long a query waits for its answer. */
const QUERY_TIMEOUT_MS = 5000

let nextRequest = 0

/** Queries waiting for their answer: request → resolve. */
const pendingQueries = new Map()

/** Server symbol kinds → VS Code's. */
const SYMBOL_KIND = {
    function: SymbolKind.Function,
    native: SymbolKind.Function,
    global: SymbolKind.Variable,
    constant: SymbolKind.Constant,
    local: SymbolKind.Variable,
    parameter: SymbolKind.Variable,
    type: SymbolKind.Class,
    library: SymbolKind.Namespace,
    struct: SymbolKind.Struct,
    interface: SymbolKind.Interface,
    module: SymbolKind.Module,
    method: SymbolKind.Method,
    field: SymbolKind.Field,
    namespace: SymbolKind.Namespace,
    class: SymbolKind.Class,
    enum: SymbolKind.Enum,
    'enum member': SymbolKind.EnumMember,
}

/**
 * Resolves the binary the extension ships, `wc3` (`wc3 server`, the
 * language backend; `wc3 mcp`, the MCP server).
 *
 * Prefers the packaged, cross-compiled binary under `./bin/` (produced by
 * `server/build.sh` / `server/build-mac.sh`: `<name>-macos` /
 * `<name>-linux` / `<name>-windows.exe`). It lives inside the extension
 * folder itself, because `vsce` only packages files under the folder that
 * holds `package.json`. Falls back to a local dev build
 * (`<crate>/target/{release,debug}/<name>`) so day-to-day development
 * doesn't require the full cross-compile.
 *
 * @param {import('vscode').ExtensionContext} context
 * @param {string} name the binary's name
 * @param {string} crateDir the crate's folder, for the dev build
 * @returns {string | null} null if the platform isn't supported.
 */
function binaryPath(context, name, crateDir) {
    const suffix = {win32: 'windows.exe', darwin: 'macos', linux: 'linux'}[process.platform]
    if (!suffix) return null

    const packaged = path.join(context.extensionPath, 'bin', `${name}-${suffix}`)
    if (fs.existsSync(packaged)) return packaged

    const devName = process.platform === 'win32' ? `${name}.exe` : name
    const release = path.join(crateDir, 'target', 'release', devName)
    if (fs.existsSync(release)) return release

    return path.join(crateDir, 'target', 'debug', devName)
}

/**
 * The one binary the extension ships, `wc3` (`wc3 server` is the language
 * backend, `wc3 mcp` the MCP server); its dev build is `../cli`'s.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function serverBinaryPath(context) {
    const found = binaryPath(context, 'wc3', path.join(context.extensionPath, '..', 'cli'))
    if (!found) window.showErrorMessage(`WC3: unsupported platform: ${process.platform}`)
    return found
}

/**
 * Registers the MCP server (`wc3 mcp`) with VS Code
 * (`contributes.mcpServerDefinitionProviders`, id `wc3`): chat agents in
 * VS Code get its tools for the game's files (SLK, MPQ, object data,
 * triggers, code ...). VS Code starts it over stdio when a chat wants it,
 * in the first workspace folder, so relative paths are the project's. The
 * same binary as the language backend (`serverBinaryPath`). Skipped where
 * VS Code has no MCP API.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerMcp(context) {
    if (!lm?.registerMcpServerDefinitionProvider || !McpStdioServerDefinition) return
    const command = binaryPath(context, 'wc3', path.join(context.extensionPath, '..', 'cli'))
    if (!command) return

    const changed = new EventEmitter()
    context.subscriptions.push(
        changed,
        // The folder it starts in follows the workspace.
        workspace.onDidChangeWorkspaceFolders(() => changed.fire()),
        lm.registerMcpServerDefinitionProvider('wc3', {
            onDidChangeMcpServerDefinitions: changed.event,
            provideMcpServerDefinitions() {
                if (!fs.existsSync(command)) return []
                const server = new McpStdioServerDefinition(
                    'WC3',
                    command,
                    ['mcp'],
                    {},
                    // A new version of the extension restarts it.
                    context.extension.packageJSON.version,
                )
                const folder = workspace.workspaceFolders?.[0]
                if (folder) server.cwd = folder.uri
                return [server]
            },
        }),
    )
}

/**
 * Is the document a file, on disk or inside a map: does it have a path?
 *
 * @param {import('vscode').Uri} uri
 */
function hasPath(uri) {
    return uri.scheme === 'file' || uri.scheme === MAP_SCHEME
}

/**
 * A document's id in the protocol: its path for a file (inside a map
 * too), so it matches the files the server finds on disk itself; its URI
 * otherwise.
 *
 * @param {import('vscode').TextDocument} doc
 */
function idOf(doc) {
    return hasPath(doc.uri) ? doc.uri.fsPath : doc.uri.toString()
}

/**
 * Is the path inside a map: does one of the folders above it name a map
 * file on disk?
 *
 * @param {string} file
 */
function inMap(file) {
    return mapOf(file) !== null
}

/**
 * The map (or game `.mpq`) a path inside one is in: the nearest folder
 * above it named like an archive that's a file on disk.
 *
 * @param {string} file
 */
function mapOf(file) {
    for (let dir = path.dirname(file); dir !== path.dirname(dir); dir = path.dirname(dir)) {
        if (!MAP_NAME.test(dir)) continue
        try {
            if (fs.statSync(dir).isFile()) return dir
        } catch {
            // Not there: keep looking above.
        }
    }
    return null
}

/**
 * The URI for an id. An untitled URI (`untitled:Untitled-1`) is never an
 * absolute path, so the two can't be confused.
 *
 * @param {string} id
 */
function uriOf(id) {
    if (!path.isAbsolute(id)) return Uri.parse(id)
    const uri = Uri.file(id)
    return inMap(id) ? uri.with({scheme: MAP_SCHEME}) : uri
}

/** @param {Buffer} frame */
function send(frame) {
    if (server && server.stdin.writable) server.stdin.write(frame)
}

/** @param {import('vscode').TextDocument} doc */
function supported(doc) {
    if (!LANGUAGES.has(doc.languageId)) return false
    if (OBJECT_LANGUAGES.has(doc.languageId)) return doc.uri.scheme === OBJECTS_SCHEME
    // A trigger's JASS (Show JASS): analyzed as any JASS without a file,
    // for its highlighting and diagnostics.
    return hasPath(doc.uri) || doc.uri.scheme === 'untitled' || SHOWN.some(s => s.scheme === doc.uri.scheme)
}

function sendRoots() {
    const roots = (workspace.workspaceFolders || [])
        .filter(f => f.uri.scheme === 'file')
        .map(f => f.uri.fsPath)
    send(encode.init(roots))
}

/** @param {import('vscode').TextDocument} doc */
function onOpen(doc) {
    const id = idOf(doc)
    const timer = pendingClose.get(id)
    if (timer !== undefined) {
        // The second half of a language change.
        clearTimeout(timer)
        pendingClose.delete(id)
        if (supported(doc)) {
            tracked.set(id, doc.languageId)
            send(encode.lang(id, doc.languageId))
        } else {
            forget(id)
        }
        return
    }
    if (!supported(doc) || tracked.has(id)) return
    tracked.set(id, doc.languageId)
    // The text form of object data names the file it shows, so the server
    // finds its project: its settings and its game's metadata.
    const file = hasPath(doc.uri) || doc.uri.scheme === OBJECTS_SCHEME ? doc.uri.fsPath : ''
    send(encode.open(id, file, doc.languageId, doc.version, doc.getText()))
}

/** @param {import('vscode').TextDocument} doc */
function onClose(doc) {
    const id = idOf(doc)
    if (!tracked.has(id)) return
    pendingClose.set(id, setTimeout(() => {
        pendingClose.delete(id)
        forget(id)
    }, 0))
}

/**
 * The editor let go of a document. Its diagnostics stay until the server
 * says otherwise: a project file keeps being reported from disk.
 *
 * @param {string} id
 */
function forget(id) {
    tracked.delete(id)
    tokens.delete(id)
    send(encode.close(id))
}

/** @param {import('vscode').TextDocumentChangeEvent} e */
function onChange(e) {
    const id = idOf(e.document)
    if (!tracked.has(id) || e.contentChanges.length === 0) return
    send(encode.edit(id, e.document.version, e.contentChanges))
}

/** @param {import('./protocol.js').ServerMessage & {kind: 'file'}} msg */
function onFile(msg) {
    // Any file: open ones and project files the editor never opened.
    diagnostics.set(uriOf(msg.id), msg.diagnostics.map(d => {
        const [sl, sc, el, ec] = d.range
        const diagnostic = new Diagnostic(new Range(sl, sc, el, ec), d.message, d.severity)
        diagnostic.source = 'wc3'
        // The lint rule, as named in `[lint]` of wc3.bni.
        if (d.rule) diagnostic.code = d.rule
        if (d.fix) fixes.set(diagnostic, d.fix)
        return diagnostic
    }))

    // A custom script's pieces open: theirs.
    if (/\.wct$/i.test(msg.id)) showPieceDiagnostics(uriOf(msg.id))

    // Highlighting only matters for what's open.
    if (!tracked.has(msg.id)) return
    tokens.set(msg.id, {version: msg.version, data: msg.tokens})
    const queue = waiting.get(msg.id) || []
    const ready = queue.filter(w => w.version <= msg.version)
    const still = queue.filter(w => w.version > msg.version)
    if (still.length) waiting.set(msg.id, still)
    else waiting.delete(msg.id)
    for (const w of ready) w.done()
    // Nobody asked for this version (a manifest change re-analyzed the
    // file, say): tell VS Code to ask again.
    if (ready.length === 0) tokensChanged.fire()
}

/**
 * Asks the server; resolves with its answer, or `null` if there's no
 * server or it doesn't answer in time.
 *
 * @param {number} kind  one of QUERY
 * @param {string} id
 * @param {{line: number, character: number}} pos
 * @param {string} [text]
 * @returns {Promise<{text: string, locations: import('./protocol.js').WireLocation[]} | null>}
 */
function query(kind, id, pos, text = '') {
    if (!server) return Promise.resolve(null)
    const request = nextRequest = (nextRequest + 1) >>> 0
    return new Promise(resolve => {
        const timer = setTimeout(() => {
            pendingQueries.delete(request)
            resolve(null)
        }, QUERY_TIMEOUT_MS)
        pendingQueries.set(request, answer => {
            clearTimeout(timer)
            resolve(answer)
        })
        send(encode.query(request, kind, id, pos, text))
    })
}

/** @param {import('./protocol.js').WireLocation} l */
function toLocation(l) {
    const [sl, sc, el, ec] = l.range
    return new Location(uriOf(l.id), new Range(sl, sc, el, ec))
}

/** @param {import('./protocol.js').WireLocation} l */
function toSymbol(l) {
    return new SymbolInformation(l.name, SYMBOL_KIND[l.kind] ?? SymbolKind.Variable, l.container, toLocation(l))
}

/**
 * Asks the server about a rename. A refusal (the server's reason, in the
 * answer's text) is thrown, which VS Code shows to the user.
 *
 * @param {number} kind  QUERY.PREPARE_RENAME or QUERY.RENAME
 * @param {import('vscode').TextDocument} doc
 * @param {import('vscode').Position} pos
 * @param {string} [newName]
 */
async function askRename(kind, doc, pos, newName = '') {
    const answer = await query(kind, ...target(doc, pos), newName)
    if (!answer) throw new Error('WC3: the language server did not answer.')
    if (answer.text) throw new Error(answer.text)
    return answer.locations
}

// ------------------------------------------------- the custom script's pieces

/**
 * A piece of a map's custom script (`war3map.wct`) open as a document of
 * its own, `wc3code:<the custom script's path>/<name>.j`: one trigger's
 * code of custom text (`?trigger`, by the trigger's name), or the map's
 * custom script (`?header`). So a trigger's code is edited without the
 * others' in reach. It's a view of those lines, not a file: the server
 * never gets it (the code would be declared twice); what it says of the
 * custom script's lines (diagnostics, highlighting, hover, go to
 * definition ...) is shown here, its lines moved. Typing in it changes
 * the custom script's text at once, and a change there (elsewhere) comes
 * here; saving it saves the custom script (and with it the triggers).
 */

/** The custom script a piece is of (its `wc3obj:` URI). @param {import('vscode').Uri} uri */
function pieceScript(uri) {
    return Uri.file(path.dirname(uri.path)).with({scheme: OBJECTS_SCHEME})
}

/** The piece of the custom script `script` (a `wc3obj:` URI) of the trigger `name`, or (`null`) the map's. */
function pieceUri(script, name) {
    const file = (name === null ? 'Map Script' : name).replace(/[\\/:*?"<>|]/g, '_') + '.j'
    return Uri.file(path.join(script.fsPath, file)).with({scheme: PIECE_SCHEME, query: name === null ? 'header' : `trigger=${encodeURIComponent(name)}`})
}

/** The open custom script's document of `uri` (a `wc3obj:` URI), if open. @param {import('vscode').Uri} uri */
function openDoc(uri) {
    const key = uri.toString().toLowerCase()
    return workspace.textDocuments.find(d => d.uri.toString().toLowerCase() === key) ?? null
}

/** A JASS string's text (`"a\"b"`), as the trigger texts write names. @param {string} s */
function jassString(s) {
    const m = /^"((?:[^"\\]|\\.)*)"/.exec(s.trim())
    if (!m) return s.trim()
    return m[1].replace(/\\(.)/g, (_, c) => ({n: '\n', r: '\r', t: '\t'})[c] ?? c)
}

/** The triggers' names in a map's triggers' text, in order. @param {string} text */
function triggerNames(text) {
    const out = []
    let inTrigger = false
    for (const line of text.split(/\r?\n/)) {
        const t = line.trim()
        if (t.startsWith('[')) {
            inTrigger = t === '[trigger]'
            if (inTrigger) out.push('')
        } else if (inTrigger && /^name\s*=/.test(t)) out[out.length - 1] = jassString(t.slice(t.indexOf('=') + 1))
    }
    return out
}

/**
 * Where the piece `uri` is in its custom script `script` (a document):
 * its code's lines `[from, to)`, its marker's line, whether it stores
 * none; `null` when it isn't there (a trigger renamed or deleted).
 */
function pieceLines(uri, script) {
    const lines = script.getText().split('\n')
    const markers = []
    lines.forEach((l, i) => {
        const m = /^\s*(?:\/\/|--)\s*@wct\s+(\S+)(?:\s+(\S+))?/.exec(l)
        if (m) markers.push({line: i, word: m[1], none: m[2] === 'none'})
    })
    const end = k => (markers[k + 1] ? markers[k + 1].line : lines.length)
    let k = -1
    if (uri.query === 'header') k = markers.findIndex(m => m.word === 'header')
    else {
        const name = decodeURIComponent(uri.query.replace(/^trigger=/, ''))
        const triggers = openDoc(sibling(script.uri, 'war3map.wtg'))
        const nth = triggers ? triggerNames(triggers.getText()).indexOf(name) : -1
        if (nth < 0) return null
        k = markers.map((m, i) => (m.word === 'trigger' ? i : -1)).filter(i => i >= 0)[nth] ?? -1
    }
    if (k < 0) return null
    return {marker: markers[k].line, from: markers[k].line + 1, to: end(k), none: markers[k].none}
}

/** The piece's text in its custom script: its code, without the last line break. */
function pieceText(uri, script) {
    const at = pieceLines(uri, script)
    if (!at) return null
    return script.getText().split('\n').slice(at.from, at.to).join('\n').replace(/\n$/, '')
}

/** Open pieces: URI → the custom script's `wc3obj:` URI. */
const pieces = new Map()

/**
 * An edit of a map's triggers and its custom script at once: the pieces
 * wait until both are done (between the two, the triggers' order and the
 * custom script's don't go together), then follow.
 * @param {import('vscode').WorkspaceEdit} edit
 */
async function applyPaired(edit) {
    pairedEdits++
    try {
        return await workspace.applyEdit(edit)
    } finally {
        pairedEdits--
        await followPieces()
    }
}
let pairedEdits = 0
/** Brings every open piece in line with its custom script (`registerPieces` sets it). */
let followPieces = async () => {}

/** A piece's document and its custom script's, when `doc` is a piece. @param {import('vscode').TextDocument} doc */
function asPiece(doc) {
    if (doc.uri.scheme !== PIECE_SCHEME) return null
    const script = openDoc(pieceScript(doc.uri))
    const at = script && pieceLines(doc.uri, script)
    return at ? {script, at} : null
}

/** What a query at `pos` in `doc` asks the server: the custom script's id and line for a piece. */
function target(doc, pos) {
    const p = asPiece(doc)
    if (!p) return [idOf(doc), pos]
    return [idOf(p.script), {line: pos.line + p.at.from, character: pos.character}]
}

/** A location the server gave, in the piece `doc` when it's in its lines. */
function backTo(doc, loc) {
    const p = asPiece(doc)
    if (!p || loc.uri.fsPath.toLowerCase() !== p.script.uri.fsPath.toLowerCase()) return loc
    const {start, end} = loc.range
    if (start.line < p.at.from || end.line >= p.at.to) return loc
    return new Location(doc.uri, new Range(start.line - p.at.from, start.character, end.line - p.at.from, end.character))
}

/**
 * The pieces: their file system (read from the custom script, saved by
 * saving it), and the text going both ways.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerPieces(context) {
    const changed = new EventEmitter()
    const stamps = new Map()
    const scriptOf = async uri => {
        const script = await workspace.openTextDocument(pieceScript(uri))
        // Its triggers, for the triggers' places by their names.
        await workspace.openTextDocument(sibling(script.uri, 'war3map.wtg')).then(undefined, () => null)
        return script
    }
    /** Puts the piece's `text` into its custom script, when it differs. */
    const putBack = async (uri, text) => {
        const script = await scriptOf(uri)
        const at = pieceLines(uri, script)
        if (!at) throw FileSystemError.FileNotFound(uri)
        const now = script.getText().split('\n').slice(at.from, at.to).join('\n').replace(/\n$/, '')
        const body = text.replace(/\r\n/g, '\n').replace(/\n$/, '')
        if (now === body) return script
        const edit = new WorkspaceEdit()
        const code = body ? body + '\n' : ''
        if (at.none && body) {
            // Code where there was none: the marker says it has some now.
            const marker = script.lineAt(at.marker).text.replace(/\s+none\s*$/, '')
            edit.replace(script.uri, new Range(at.marker, 0, at.to, 0), marker + '\n' + code)
        } else edit.replace(script.uri, new Range(at.from, 0, at.to, 0), code)
        await workspace.applyEdit(edit)
        return script
    }
    const provider = {
        onDidChangeFile: changed.event,
        watch: () => new Disposable(() => {}),
        stat: async uri => {
            const text = pieceText(uri, await scriptOf(uri))
            if (text === null) throw FileSystemError.FileNotFound(uri)
            return {type: FileType.File, ctime: 0, mtime: stamps.get(uri.toString()) ?? 0, size: Buffer.byteLength(text)}
        },
        readFile: async uri => {
            const text = pieceText(uri, await scriptOf(uri))
            if (text === null) throw FileSystemError.FileNotFound(uri)
            return Buffer.from(text, 'utf8')
        },
        // Saving a piece saves the custom script it's of. One that isn't
        // found any more (its trigger renamed or gone) has nothing to put
        // back: what was typed in it is there already.
        writeFile: async (uri, content) => {
            const found = pieceLines(uri, await scriptOf(uri))
            if (!found) return void stamps.set(uri.toString(), Date.now())
            const script = await putBack(uri, Buffer.from(content).toString('utf8'))
            if (script.isDirty) await script.save()
            stamps.set(uri.toString(), Date.now())
        },
        readDirectory: () => [],
        createDirectory: uri => { throw FileSystemError.NoPermissions(uri) },
        delete: uri => { throw FileSystemError.NoPermissions(uri) },
        rename: uri => { throw FileSystemError.NoPermissions(uri) },
    }
    let syncing = false
    /** Each open piece of the custom script `key` (or every one) given its text there. */
    const follow = async (key = null) => {
        for (const piece of workspace.textDocuments.filter(d => d.uri.scheme === PIECE_SCHEME)) {
            const script = pieceScript(piece.uri)
            if (key && script.toString().toLowerCase() !== key && sibling(script, 'war3map.wtg').toString().toLowerCase() !== key) continue
            const open = openDoc(script)
            const text = open && pieceText(piece.uri, open)
            if (text === null || text === undefined || text === piece.getText().replace(/\r\n/g, '\n').replace(/\n$/, '')) continue
            const edit = new WorkspaceEdit()
            edit.replace(piece.uri, new Range(0, 0, piece.lineCount, 0), text)
            syncing = true
            try { await workspace.applyEdit(edit) } finally { syncing = false }
        }
    }
    followPieces = () => follow()
    context.subscriptions.push(
        changed,
        workspace.registerFileSystemProvider(PIECE_SCHEME, provider, {isCaseSensitive: process.platform === 'linux'}),
        workspace.onDidOpenTextDocument(doc => {
            if (doc.uri.scheme === PIECE_SCHEME) pieces.set(doc.uri.toString(), pieceScript(doc.uri))
        }),
        workspace.onDidCloseTextDocument(doc => pieces.delete(doc.uri.toString())),
        workspace.onDidChangeTextDocument(async e => {
            if (syncing || !e.contentChanges.length) return
            const doc = e.document
            // Typed in a piece: into the custom script.
            if (doc.uri.scheme === PIECE_SCHEME) {
                syncing = true
                try { await putBack(doc.uri, doc.getText()) } catch { /* gone: the piece stays as it is */ } finally { syncing = false }
                return
            }
            // The custom script (or its triggers) changed elsewhere: its
            // pieces open follow, unless it's half of an edit of both.
            const triggers = /war3map\.wtg$/i.test(doc.uri.path)
            if (!triggers && !/war3map\.wct$/i.test(doc.uri.path)) return
            if (pairedEdits === 0) await follow(doc.uri.toString().toLowerCase())
            // Its diagnostics moved with it.
            if (!triggers) showPieceDiagnostics(doc.uri)
        }),
    )
}

/** The diagnostics of the custom script `script` (a URI) in its open pieces, their lines moved. */
function showPieceDiagnostics(script) {
    const key = script.fsPath.toLowerCase()
    for (const piece of workspace.textDocuments.filter(d => d.uri.scheme === PIECE_SCHEME && pieceScript(d.uri).fsPath.toLowerCase() === key)) {
        const p = asPiece(piece)
        if (!p) continue
        // The custom script's text (`wc3obj:`) is what the server analyzes.
        const all = diagnostics.get(Uri.file(script.fsPath).with({scheme: OBJECTS_SCHEME})) ?? []
        const own = all.filter(d => d.range.start.line >= p.at.from && d.range.end.line < p.at.to).map(d => {
            const moved = new Diagnostic(new Range(d.range.start.line - p.at.from, d.range.start.character, d.range.end.line - p.at.from, d.range.end.character), d.message, d.severity)
            moved.source = d.source
            moved.code = d.code
            return moved
        })
        diagnostics.set(piece.uri, own)
    }
}

/**
 * Go to definition, find references, hover, outline, workspace symbol
 * search and rename, all answered by the server's symbol index.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerQueries(context) {
    const selector = [...LANGUAGES].map(language => ({language}))
    const ask = async (kind, doc, pos) => (await query(kind, ...target(doc, pos)))?.locations ?? []
    const located = (doc, list) => list.map(toLocation).map(l => backTo(doc, l))
    context.subscriptions.push(
        languages.registerDefinitionProvider(selector, {
            provideDefinition: async (doc, pos) => located(doc, await ask(QUERY.DEFINITION, doc, pos)),
        }),
        languages.registerReferenceProvider(selector, {
            provideReferences: async (doc, pos, ctx) => {
                const found = located(doc, await ask(QUERY.REFERENCES, doc, pos))
                if (ctx.includeDeclaration) return found
                const decls = located(doc, await ask(QUERY.DEFINITION, doc, pos))
                return found.filter(r => !decls.some(d => d.uri.toString() === r.uri.toString() && d.range.isEqual(r.range)))
            },
        }),
        languages.registerHoverProvider(selector, {
            provideHover: async (doc, pos) => {
                const answer = await query(QUERY.HOVER, ...target(doc, pos))
                if (!answer || !answer.text) return null
                const text = new MarkdownString(answer.text)
                // A string table's colors come as <span style="color:...">.
                text.supportHtml = true
                return new Hover(text)
            },
        }),
        languages.registerDocumentSymbolProvider(selector, {
            provideDocumentSymbols: async doc => {
                const all = (await ask(QUERY.DOCUMENT_SYMBOLS, doc, {line: 0, character: 0})).map(toSymbol)
                if (doc.uri.scheme !== PIECE_SCHEME) return all
                // A piece's own, in its lines.
                return all.map(s => ({s, l: backTo(doc, s.location)})).filter(x => x.l.uri.scheme === PIECE_SCHEME)
                    .map(({s, l}) => new SymbolInformation(s.name, s.kind, s.containerName, l))
            },
        }),
        languages.registerRenameProvider(selector, {
            prepareRename: async (doc, pos) => {
                const [here] = await askRename(QUERY.PREPARE_RENAME, doc, pos)
                if (!here) throw new Error('There\'s no symbol here to rename.')
                return {range: backTo(doc, toLocation(here)).range, placeholder: here.name}
            },
            provideRenameEdits: async (doc, pos, newName) => {
                let locations = (await askRename(QUERY.RENAME, doc, pos, newName)).map(toLocation).map(l => backTo(doc, l))
                // Changing a file inside a map rewrites the map: ask first.
                const maps = new Set(locations.filter(l => l.uri.scheme === MAP_SCHEME).map(l => mapOf(l.uri.fsPath)))
                if (maps.size) {
                    const inside = locations.filter(l => l.uri.scheme === MAP_SCHEME).length
                    const names = [...maps].map(m => path.basename(m)).join(', ')
                    const rename = 'Rename'
                    const outside = 'Rename Outside Maps'
                    const choice = await window.showWarningMessage(
                        `You are changing ${maps.size === 1 ? 'an archive' : 'archives'}!`,
                        {
                            modal: true,
                            detail: `${inside} of the ${locations.length} places to rename are inside ${names}. ` +
                                'Renaming there rewrites the map file.',
                        },
                        rename,
                        outside,
                    )
                    if (!choice) return new WorkspaceEdit()
                    if (choice === outside) locations = locations.filter(l => l.uri.scheme !== MAP_SCHEME)
                }
                const edit = new WorkspaceEdit()
                for (const loc of locations) edit.replace(loc.uri, loc.range, newName)
                return edit
            },
        }),
        languages.registerWorkspaceSymbolProvider({
            provideWorkspaceSymbols: async text =>
                ((await query(QUERY.WORKSPACE_SYMBOLS, '', {line: 0, character: 0}, text))?.locations ?? []).map(toSymbol),
        }),
    )
}

/**
 * Quick fixes: the ones the server sent with the diagnostics at the
 * cursor (removing an empty `else`, say).
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerFixes(context) {
    const selector = [...LANGUAGES].map(language => ({language}))
    context.subscriptions.push(languages.registerCodeActionsProvider(selector, {
        provideCodeActions(doc, _range, ctx) {
            // VS Code hands over copies: find ours by range and message.
            const ours = diagnostics.get(doc.uri) ?? []
            return ctx.diagnostics.flatMap(diagnostic => {
                const same = ours.find(o => fixes.has(o) && o.range.isEqual(diagnostic.range) && o.message === diagnostic.message)
                const fix = same && fixes.get(same)
                if (!fix) return []
                const action = new CodeAction(fix.title, CodeActionKind.QuickFix)
                action.edit = new WorkspaceEdit()
                for (const {range, text} of fix.edits) action.edit.replace(doc.uri, new Range(...range), text)
                action.diagnostics = [diagnostic]
                action.isPreferred = true
                return [action]
            })
        },
    }, {providedCodeActionKinds: [CodeActionKind.QuickFix]}))
}

/** The command a path link runs: reveals a folder. */
const REVEAL_FOLDER = 'wc3.revealFolder'

/**
 * Path completion and folder links in `wc3.bni` (see the header).
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerManifestPaths(context) {
    const manifest = {language: 'bni', pattern: '**/wc3.bni'}
    const zero = {line: 0, character: 0}
    const toRange = l => new Range(...l.range)
    context.subscriptions.push(
        commands.registerCommand(REVEAL_FOLDER, folder => {
            const uri = Uri.file(folder)
            return commands.executeCommand(workspace.getWorkspaceFolder(uri) ? 'revealInExplorer' : 'revealFileInOS', uri)
        }),
        languages.registerCompletionItemProvider(manifest, {
            provideCompletionItems: async (doc, pos) => {
                const answer = await query(QUERY.PATH_COMPLETION, idOf(doc), pos)
                return (answer?.locations ?? []).map(l => {
                    const folder = l.container === 'folder'
                    const item = new CompletionItem(l.name, folder ? CompletionItemKind.Folder : CompletionItemKind.File)
                    item.range = toRange(l)
                    item.detail = l.id
                    if (folder) {
                        // Keep going down: the next folder's entries.
                        item.insertText = l.name + '/'
                        item.command = {command: 'editor.action.triggerSuggest', title: 'Suggest'}
                    }
                    return item
                })
            },
        }, '/', '\\', '=', '"'),
        languages.registerDocumentLinkProvider(manifest, {
            provideDocumentLinks: async doc => {
                const answer = await query(QUERY.PATH_LINKS, idOf(doc), zero)
                return (answer?.locations ?? []).map(l => {
                    const args = encodeURIComponent(JSON.stringify([l.id]))
                    const link = new DocumentLink(toRange(l), Uri.parse(`command:${REVEAL_FOLDER}?${args}`))
                    link.tooltip = `Reveal ${l.id}`
                    return link
                })
            },
        }),
    )
}

/** The id of the list of maps (`package.json`). */
const MAPS_VIEW = 'wc3.maps'

/** Maps the list shows. */
const MAP_GLOB = '**/*.{w3x,w3m,W3X,W3M}'

/** The command that opens a map as a workspace folder. */
const OPEN_MAP = 'wc3.openMapAsFolder'

/** How long a file system operation may take: writing rebuilds a map. */
const FS_TIMEOUT_MS = 60000

/**
 * A file system operation inside a map, answered by the server (see
 * `server/src/mapfs.rs`). Rejects with the matching `FileSystemError`.
 *
 * @param {number} op  one of FS_OP
 * @param {import('vscode').Uri} uri
 * @param {{to?: import('vscode').Uri, flags?: number, data?: Uint8Array}} [more]
 */
function fsRequest(op, uri, more = {}) {
    if (!server) return Promise.reject(FileSystemError.Unavailable(uri))
    const request = nextRequest = (nextRequest + 1) >>> 0
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            pendingQueries.delete(request)
            reject(FileSystemError.Unavailable(uri))
        }, FS_TIMEOUT_MS)
        pendingQueries.set(request, answer => {
            clearTimeout(timer)
            if (!answer.error) return resolve(answer)
            const make = FileSystemError[answer.error] ?? FileSystemError.Unavailable
            reject(answer.message ? make(`${uri.fsPath}: ${answer.message}`) : make(uri))
        })
        send(encode.fs(request, op, uri.fsPath, more.to?.fsPath ?? '', more.flags ?? 0, more.data ?? new Uint8Array()))
    })
}

/** The `wc3map:` URI of a map file, the folder it opens as. */
function mapFolderUri(map) {
    return Uri.file(map).with({scheme: MAP_SCHEME})
}

/**
 * Maps as folders (see the header): the file system under `wc3map:`, the
 * command that adds a map to the workspace, the editor a map file opens
 * in, notes on files that don't count, and the list of maps.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerMaps(context) {
    const changed = new EventEmitter()
    const fire = (type, ...uris) => changed.fire(uris.map(uri => ({type, uri})))
    const parent = uri => uri.with({path: path.posix.dirname(uri.path)})

    const provider = {
        onDidChangeFile: changed.event,
        // The server notices changes to maps itself; see the watcher below.
        watch: () => new Disposable(() => {}),

        async stat(uri) {
            const {stat} = await fsRequest(FS_OP.STAT, uri)
            return {
                type: stat.type === 'dir' ? FileType.Directory : FileType.File,
                ctime: stat.mtime,
                mtime: stat.mtime,
                size: stat.size,
                permissions: stat.readonly ? FilePermission.Readonly : undefined,
            }
        },
        async readDirectory(uri) {
            const {entries} = await fsRequest(FS_OP.READ_DIR, uri)
            return entries.map(([name, type]) => [name, type === 'dir' ? FileType.Directory : FileType.File])
        },
        async readFile(uri) {
            return (await fsRequest(FS_OP.READ, uri)).data
        },
        async writeFile(uri, data, options) {
            const exists = await provider.stat(uri).then(() => true, () => false)
            const flags = (options.create ? FS_FLAG.CREATE : 0) | (options.overwrite ? FS_FLAG.OVERWRITE : 0)
            await fsRequest(FS_OP.WRITE, uri, {flags, data})
            if (exists) fire(FileChangeType.Changed, uri)
            else fire(FileChangeType.Created, uri)
        },
        async delete(uri, options) {
            await fsRequest(FS_OP.DELETE, uri, {flags: options.recursive ? FS_FLAG.RECURSIVE : 0})
            fire(FileChangeType.Deleted, uri)
        },
        async rename(from, to, options) {
            await fsRequest(FS_OP.RENAME, from, {to, flags: options.overwrite ? FS_FLAG.OVERWRITE : 0})
            changed.fire([{type: FileChangeType.Deleted, uri: from}, {type: FileChangeType.Created, uri: to}])
        },
        async createDirectory(uri) {
            await fsRequest(FS_OP.CREATE_DIR, uri)
            fire(FileChangeType.Created, uri)
        },
    }

    // A file a map holds but the game doesn't use, or a map that can't be
    // written: said next to it.
    const decorationsChanged = new EventEmitter()
    const decorations = {
        onDidChangeFileDecorations: decorationsChanged.event,
        async provideFileDecoration(uri) {
            if (uri.scheme !== MAP_SCHEME) return undefined
            const stat = await fsRequest(FS_OP.STAT, uri).then(a => a.stat, () => null)
            if (!stat?.note) return undefined
            const unused = stat.type === 'file'
            const decoration = new FileDecoration(unused ? '−' : undefined, stat.note)
            if (unused) decoration.color = new ThemeColor('disabledForeground')
            return decoration
        },
    }

    /** Adds the map to the workspace, or shows it if it's there. */
    const openMap = target => {
        const map = target instanceof Uri ? target.fsPath : target?.map ?? window.activeTextEditor?.document.uri.fsPath
        if (!map || !MAP_NAME.test(map)) return
        const uri = mapFolderUri(map)
        const folders = workspace.workspaceFolders ?? []
        if (folders.some(f => f.uri.toString() === uri.toString())) {
            return commands.executeCommand('revealInExplorer', uri)
        }
        workspace.updateWorkspaceFolders(folders.length, 0, {uri, name: path.basename(map)})
    }

    // A map file opens as a page that offers to open it as a folder.
    const editor = {
        openCustomDocument: uri => ({uri, dispose() {}}),
        resolveCustomEditor(document, panel) {
            const map = document.uri.fsPath
            const isOpen = (workspace.workspaceFolders ?? []).some(f => f.uri.toString() === mapFolderUri(map).toString())
            panel.webview.options = {enableScripts: true}
            panel.iconPath = mapIcon(map)
            const size = (() => {
                try {
                    return `${(fs.statSync(map).size / 1048576).toFixed(1)} MB`
                } catch {
                    return ''
                }
            })()
            const html = s => s.replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'})[c])
            panel.webview.html = `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'">
<style>
body { font-family: var(--vscode-font-family); color: var(--vscode-foreground); padding: 2em; }
h1 { font-size: 1.4em; font-weight: 600; margin: 0 0 .3em; }
p { color: var(--vscode-descriptionForeground); margin: 0 0 1.5em; }
button { background: var(--vscode-button-background); color: var(--vscode-button-foreground); border: 0; padding: .5em 1.2em; font: inherit; cursor: pointer; border-radius: 2px; }
button:hover { background: var(--vscode-button-hoverBackground); }
</style></head><body>
<h1>${html(path.basename(map))}</h1>
<p>${html(path.dirname(map))}${size ? ' · ' + size : ''}</p>
<button id="open">${isOpen ? 'Show in Explorer' : 'Open as Folder'}</button>
<script>
const vscode = acquireVsCodeApi()
document.getElementById('open').onclick = () => vscode.postMessage('open')
</script></body></html>`
            panel.webview.onDidReceiveMessage(() => openMap(document.uri))
        },
    }

    // The list of maps in the workspace; a click opens one as a folder.
    const listChanged = new EventEmitter()
    const byName = (a, b) => a.localeCompare(b, undefined, {sensitivity: 'base'})
    /** A map's icon: its own for `.w3m`, `.w3x`'s otherwise. */
    const mapIcon = map => Uri.file(path.join(context.extensionPath, 'icons', /\.w3m$/i.test(map) ? 'w3m.svg' : 'w3x.svg'))
    const list = {
        onDidChangeTreeData: listChanged.event,
        async getChildren(node) {
            if (node) return []
            const found = await workspace.findFiles(MAP_GLOB)
            return found
                .map(uri => uri.fsPath)
                .sort((a, b) => byName(path.basename(a), path.basename(b)) || byName(a, b))
                .map(map => ({map}))
        },
        getTreeItem({map}) {
            const item = new TreeItem(path.basename(map), TreeItemCollapsibleState.None)
            item.id = map
            item.iconPath = mapIcon(map)
            item.description = workspace.asRelativePath(path.dirname(map))
            item.tooltip = map
            item.resourceUri = Uri.file(map)
            item.contextValue = 'wc3.map'
            item.command = {command: OPEN_MAP, title: 'Open as Folder', arguments: [Uri.file(map)]}
            return item
        },
    }

    // A map changed on disk (saved by the World Editor, or by us): what's
    // shown of it is read again.
    const mapChanged = uri => {
        const root = mapFolderUri(uri.fsPath)
        const inside = workspace.textDocuments
            .map(d => d.uri)
            .filter(u => u.scheme === MAP_SCHEME && u.fsPath.toLowerCase().startsWith(uri.fsPath.toLowerCase() + path.sep))
        fire(FileChangeType.Changed, root, ...inside)
        decorationsChanged.fire(undefined)
    }
    const listRefresh = () => listChanged.fire(undefined)
    const watcher = workspace.createFileSystemWatcher(MAP_GLOB)

    context.subscriptions.push(
        changed,
        decorationsChanged,
        listChanged,
        watcher,
        workspace.registerFileSystemProvider(MAP_SCHEME, provider, {isCaseSensitive: false}),
        window.registerFileDecorationProvider(decorations),
        window.registerCustomEditorProvider('wc3.map', editor, {supportsMultipleEditorsPerDocument: true}),
        window.createTreeView(MAPS_VIEW, {treeDataProvider: list}),
        commands.registerCommand(OPEN_MAP, openMap),
        commands.registerCommand('wc3.maps.refresh', listRefresh),
        commands.registerCommand('wc3.maps.reveal', node => node?.map && commands.executeCommand('revealInExplorer', Uri.file(node.map))),
        watcher.onDidChange(mapChanged),
        watcher.onDidCreate(listRefresh),
        watcher.onDidDelete(listRefresh),
        workspace.onDidChangeWorkspaceFolders(listRefresh),
    )
}

/**
 * Asks the server to decode a texture file's bytes; resolves with the
 * decoded texture (whose `error` says why when it isn't one).
 *
 * @param {string} name  the file's path
 * @param {Uint8Array} data
 * @returns {Promise<import('./protocol.js').Texture>}
 */
function textureRequest(name, data) {
    if (!server) return Promise.reject(new Error('The WC3 server isn\'t running.'))
    const request = nextRequest = (nextRequest + 1) >>> 0
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            pendingQueries.delete(request)
            reject(new Error('The WC3 server didn\'t answer in time.'))
        }, FS_TIMEOUT_MS)
        pendingQueries.set(request, answer => {
            clearTimeout(timer)
            resolve(answer.texture)
        })
        send(encode.texture(request, name, data))
    })
}

/**
 * The texture viewer (see the header): the editor `.blp` files open in.
 * The page (`webview/texture.js`) asks for the texture once it's loaded,
 * and again whenever it's shown after being hidden; the file is read
 * again when it changes.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerTextures(context) {
    const media = Uri.joinPath(context.extensionUri, 'webview')

    /** The page, its script and style from `webview/`. */
    const html = webview => {
        const nonce = [...Array(32)].map(() => Math.floor(Math.random() * 36).toString(36)).join('')
        const asset = name => webview.asWebviewUri(Uri.joinPath(media, name))
        return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}'">
<link rel="stylesheet" href="${asset('texture.css')}">
</head><body>
<div id="loading" class="message"><p>Decoding…</p></div>
<div id="error" class="message" hidden><h2>Can't show this texture</h2><p id="error-text"></p></div>
<div id="viewer" hidden>
  <div class="bar">
    <div class="group" id="mips"><span class="label">Mip</span></div>
    <div class="group" id="channels"><span class="label" id="channels-label">Channels</span></div>
    <div class="group" id="background"><span class="label">Background</span></div>
    <div class="group">
      <button id="zoom-out" title="Zoom out (−)">−</button>
      <span id="zoom-value" class="zoom-value"></span>
      <button id="zoom-in" title="Zoom in (+)">+</button>
      <button id="zoom-fit" title="Fit (0)">Fit</button>
      <button id="zoom-1" title="Actual size (1)">1:1</button>
    </div>
    <div class="spacer"></div>
    <span id="info" class="info"></span>
    <button id="save" title="Save this mip as PNG">Save PNG</button>
  </div>
  <div id="stage" class="stage">
    <div id="frame" class="frame checker"><canvas id="canvas"></canvas></div>
    <div id="missing" class="message" hidden><p id="missing-text"></p></div>
  </div>
  <div class="status">
    <span id="pixel" class="pixel" hidden>
      <span id="pos"></span><span id="swatch" class="swatch"></span><span id="rgba"></span><span id="hex"></span>
    </span>
  </div>
</div>
<script nonce="${nonce}" src="${asset('texture.js')}"></script>
</body></html>`
    }

    /** Saves a mip's PNG (encoded by the page), next to the texture or its map. */
    const saveMip = async (uri, mip, png) => {
        const base = path.posix.basename(uri.path).replace(/\.[^.]+$/, '')
        const name = mip ? `${base}_mip${mip}.png` : `${base}.png`
        const file = uri.scheme === MAP_SCHEME ? mapOf(uri.fsPath) : uri.scheme === 'file' ? uri.fsPath : null
        const target = await window.showSaveDialog({
            defaultUri: file ? Uri.file(path.join(path.dirname(file), name)) : undefined,
            filters: {'PNG image': ['png']},
        })
        if (!target) return
        await workspace.fs.writeFile(target, png)
        window.setStatusBarMessage(`Saved ${workspace.asRelativePath(target)}`, 3000)
    }

    const editor = {
        openCustomDocument: uri => ({uri, dispose() {}}),
        resolveCustomEditor(document, panel) {
            const {uri} = document
            const {webview} = panel
            webview.options = {enableScripts: true, localResourceRoots: [media]}
            const kind = /\.(wpm|tga|shd)$/i.exec(uri.path)?.[1].toLowerCase() ?? 'blp'
            panel.iconPath = Uri.joinPath(context.extensionUri, 'icons', `${kind}.svg`)
            webview.html = html(webview)

            const load = async () => {
                try {
                    const data = await workspace.fs.readFile(uri)
                    webview.postMessage({type: 'texture', size: data.length, texture: await textureRequest(uri.fsPath, data)})
                } catch (e) {
                    webview.postMessage({type: 'error', message: e instanceof Error ? e.message : String(e)})
                }
            }
            const dir = uri.with({path: path.posix.dirname(uri.path)})
            const watcher = workspace.createFileSystemWatcher(new RelativePattern(dir, path.posix.basename(uri.path)))
            const subscriptions = [
                watcher,
                watcher.onDidChange(load),
                watcher.onDidCreate(load),
                webview.onDidReceiveMessage(msg => {
                    if (msg.type === 'ready') load()
                    else if (msg.type === 'save') saveMip(uri, msg.mip, msg.png).catch(e => window.showErrorMessage(`WC3: ${e.message}`))
                }),
            ]
            panel.onDidDispose(() => subscriptions.forEach(d => d.dispose()))
        },
    }
    context.subscriptions.push(window.registerCustomEditorProvider('wc3.blp', editor, {supportsMultipleEditorsPerDocument: true}))
}

/**
 * Has the server convert object data (see the header): a file's bytes to
 * its text form (`OBJECTS_OP.TO_TEXT`), or a text form back to the bytes.
 * Rejects with the server's reason when it can't.
 *
 * @param {number} op  one of OBJECTS_OP
 * @param {string} name  the file's path; its extension says what it holds
 * @param {Uint8Array} data
 * @returns {Promise<Uint8Array>}
 */
function objectsRequest(op, name, data) {
    if (!server) return Promise.reject(new Error('The WC3 server isn\'t running.'))
    const request = nextRequest = (nextRequest + 1) >>> 0
    return new Promise((resolve, reject) => {
        const timer = setTimeout(() => {
            pendingQueries.delete(request)
            reject(new Error('The WC3 server didn\'t answer in time.'))
        }, FS_TIMEOUT_MS)
        pendingQueries.set(request, answer => {
            clearTimeout(timer)
            if (answer.error) reject(new Error(answer.error))
            else resolve(answer.data)
        })
        send(encode.objects(request, op, name, data))
    })
}

/**
 * Object data as text (see the header): the `wc3obj:` file system over
 * the real files, and the editor `.w3a` ... files open in, which opens
 * their text instead.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerObjects(context) {
    const changed = new EventEmitter()
    /** The file a `wc3obj:` URI shows: on disk, or inside a map. */
    const source = uri => {
        const file = Uri.file(uri.fsPath)
        return inMap(uri.fsPath) ? file.with({scheme: MAP_SCHEME}) : file
    }
    /** The file on disk that changes with it: itself, or its map. */
    const onDisk = uri => Uri.file(mapOf(uri.fsPath) ?? uri.fsPath)
    const failed = (uri, e) => FileSystemError.Unavailable(`${path.basename(uri.fsPath)}: ${e instanceof Error ? e.message : e}`)

    const provider = {
        onDidChangeFile: changed.event,
        watch(uri) {
            const file = onDisk(uri)
            const dir = Uri.file(path.dirname(file.fsPath))
            const watcher = workspace.createFileSystemWatcher(new RelativePattern(dir, path.basename(file.fsPath)))
            const fire = () => changed.fire([{type: FileChangeType.Changed, uri}])
            const subscriptions = [watcher, watcher.onDidChange(fire), watcher.onDidCreate(fire)]
            return new Disposable(() => subscriptions.forEach(d => d.dispose()))
        },
        async stat(uri) {
            const stat = await workspace.fs.stat(source(uri))
            if (stat.type !== FileType.File) throw FileSystemError.FileNotFound(uri)
            return stat
        },
        readDirectory: () => [],
        async readFile(uri) {
            const data = await workspace.fs.readFile(source(uri))
            try {
                return await objectsRequest(OBJECTS_OP.TO_TEXT, uri.fsPath, data)
            } catch (e) {
                throw failed(uri, e)
            }
        },
        async writeFile(uri, content) {
            let data
            try {
                data = await objectsRequest(OBJECTS_OP.TO_BYTES, uri.fsPath, content)
            } catch (e) {
                throw failed(uri, e)
            }
            await workspace.fs.writeFile(source(uri), data)
            changed.fire([{type: FileChangeType.Changed, uri}])
        },
        delete: uri => {
            throw FileSystemError.NoPermissions(uri)
        },
        rename: uri => {
            throw FileSystemError.NoPermissions(uri)
        },
        createDirectory: uri => {
            throw FileSystemError.NoPermissions(uri)
        },
    }

    // A `.w3a` ... file opens its text in a text editor, in place of this
    // one; when it can't, this one says why.
    const editor = {
        openCustomDocument: uri => ({uri, dispose() {}}),
        async resolveCustomEditor(document, panel) {
            const text = Uri.file(document.uri.fsPath).with({scheme: OBJECTS_SCHEME})
            const html = s => s.replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'})[c])
            panel.webview.html = `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<style>body { font-family: var(--vscode-font-family); color: var(--vscode-descriptionForeground); padding: 2em; }</style>
</head><body><p>Opening ${html(path.basename(document.uri.fsPath))} as text…</p></body></html>`
            try {
                const doc = await workspace.openTextDocument(text)
                // A map's triggers: as World Editor shows them (Open as Text for the text).
                // Its custom script too, with its triggers beside it: the code of
                // custom text is theirs (Map Script in the tree opens the text).
                const triggers = /^war3map\.wct$/i.test(path.basename(document.uri.fsPath))
                    ? await workspace.openTextDocument(sibling(text, 'war3map.wtg')).then(d => d, () => null)
                    : null
                if (doc.languageId === 'wtg') await commands.executeCommand('vscode.openWith', text, 'wc3.wtgTriggers', {viewColumn: panel.viewColumn, preview: false})
                else if (triggers) await commands.executeCommand('vscode.openWith', triggers.uri, 'wc3.wtgTriggers', {viewColumn: panel.viewColumn, preview: false})
                else await window.showTextDocument(doc, {viewColumn: panel.viewColumn, preview: false})
                panel.dispose()
            } catch (e) {
                const message = e instanceof Error ? e.message : String(e)
                panel.webview.html = `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'">
<style>body { font-family: var(--vscode-font-family); color: var(--vscode-foreground); padding: 2em; }
h2 { font-size: 1.2em; font-weight: 600; } p { color: var(--vscode-errorForeground); }</style>
</head><body><h2>Can't show ${html(path.basename(document.uri.fsPath))} as text</h2><p>${html(message)}</p></body></html>`
            }
        },
    }

    // Each field's name and type after it: inlay hints, shown but not part
    // of the text. Asked for the lines in view and as many again above and
    // below (50 at least), so scrolling finds them ready; a whole file's
    // would slow the editor down.
    const hints = {
        async provideInlayHints(doc, range) {
            const margin = Math.max(range.end.line - range.start.line, 50)
            const first = Math.max(0, range.start.line - margin)
            const last = Math.min(doc.lineCount - 1, range.end.line + margin)
            const answer = await query(QUERY.INLAY_HINTS, idOf(doc), {line: first, character: last})
            if (!answer || !answer.text) return []
            return JSON.parse(answer.text).map(([line, character, label]) => {
                const hint = new InlayHint(new Position(line, character), label)
                hint.paddingLeft = true
                return hint
            })
        },
    }
    const selector = [...OBJECT_LANGUAGES].map(language => ({language, scheme: OBJECTS_SCHEME}))

    // A region's or a minimap icon's `color=red,green,blue,alpha` as a
    // color: a swatch before
    // it that opens the color picker. The server finds the values; a
    // picked color is written back as four numbers from 0 to 255.
    const byte = v => Math.round(Math.min(Math.max(v, 0), 1) * 255)
    const colors = {
        async provideDocumentColors(doc) {
            const answer = await query(QUERY.COLORS, idOf(doc), {line: 0, character: 0})
            if (!answer || !answer.text) return []
            return JSON.parse(answer.text).map(([line, character, endLine, endCharacter, r, g, b, a]) =>
                new ColorInformation(
                    new Range(line, character, endLine, endCharacter),
                    new Color(r / 255, g / 255, b / 255, a / 255),
                ))
        },
        provideColorPresentations(color) {
            const label = [color.red, color.green, color.blue, color.alpha].map(byte).join(',')
            return [new ColorPresentation(label)]
        },
    }

    context.subscriptions.push(
        changed,
        languages.registerInlayHintsProvider(selector, hints),
        languages.registerColorProvider(['w3r', 'mmp'].map(language => ({language, scheme: OBJECTS_SCHEME})), colors),
        workspace.registerFileSystemProvider(OBJECTS_SCHEME, provider, {isCaseSensitive: process.platform === 'linux'}),
        window.registerCustomEditorProvider('wc3.objects', editor, {supportsMultipleEditorsPerDocument: true}),
    )
}

/**
 * Highlighting for every language, served from the tokens the server
 * pushed. If the document is newer than those, waits (briefly) for the
 * server's answer to the edit instead of showing stale colors.
 *
 * @param {import('./protocol.js').ServerMessage & {kind: 'legend'}} msg
 * @param {import('vscode').ExtensionContext} context
 */
/**
 * The tokens (VS Code's encoding: five numbers each, the line and start
 * relative to the one before) of lines `[from, to)`, those lines counted
 * from 0.
 *
 * @param {ArrayLike<number>} data
 * @param {number} from
 * @param {number} to
 */
function sliceTokens(data, from, to) {
    const out = []
    let line = 0
    let start = 0
    let lastLine = 0
    let lastStart = 0
    for (let i = 0; i + 4 < data.length; i += 5) {
        line += data[i]
        start = data[i] === 0 ? start + data[i + 1] : data[i + 1]
        if (line < from || line >= to) continue
        const l = line - from
        out.push(l - lastLine, l === lastLine ? start - lastStart : start, data[i + 2], data[i + 3], data[i + 4])
        lastLine = l
        lastStart = start
    }
    return new Uint32Array(out)
}

function registerTokens(msg, context) {
    const legend = new SemanticTokensLegend(msg.types, msg.modifiers)
    const provider = {
        onDidChangeSemanticTokens: tokensChanged.event,

        /** @param {import('vscode').TextDocument} doc */
        provideDocumentSemanticTokens(doc) {
            // A piece: its custom script's tokens of its lines.
            if (doc.uri.scheme === PIECE_SCHEME) {
                const p = asPiece(doc)
                const t = p && tokens.get(idOf(p.script))
                return t ? new SemanticTokens(sliceTokens(t.data, p.at.from, p.at.to)) : null
            }
            const id = idOf(doc)
            const latest = () => {
                const t = tokens.get(id)
                return t ? new SemanticTokens(t.data) : null
            }
            const cached = tokens.get(id)
            if ((cached && cached.version >= doc.version) || !tracked.has(id)) return latest()
            return new Promise(resolve => {
                const waiter = {version: doc.version, done: () => (clearTimeout(timer), resolve(latest()))}
                // Gave up waiting: answer with what there is, and leave the
                // queue, so the tokens arriving later count as unasked for
                // and make VS Code ask again (see onFile).
                const timer = setTimeout(() => {
                    const queue = (waiting.get(id) || []).filter(w => w !== waiter)
                    if (queue.length) waiting.set(id, queue)
                    else waiting.delete(id)
                    resolve(latest())
                }, TOKENS_WAIT_MS)
                const queue = waiting.get(id) || []
                queue.push(waiter)
                waiting.set(id, queue)
            })
        },
    }
    const selector = [...LANGUAGES].map(language => ({language}))
    context.subscriptions.push(languages.registerDocumentSemanticTokensProvider(selector, provider, legend))
}

/**
 * @param {string} command
 * @param {import('vscode').ExtensionContext} context
 */
function startServer(command, context) {
    server = spawn(command, ['server'], {stdio: ['pipe', 'pipe', 'pipe']})
    server.on('error', e => {
        window.showErrorMessage(
            `WC3: failed to start the language server (${e.message}). ` +
            'Run "cargo build" in cli/ (or server/build-mac.sh, server/build.sh for packaged binaries) first.',
        )
        server = null
    })
    server.on('exit', code => {
        if (server) console.error(`[wc3] server exited with code ${code}`)
        server = null
    })
    server.stderr.on('data', chunk => console.error(`[wc3 server] ${chunk.toString().trimEnd()}`))

    const frames = new FrameReader()
    server.stdout.on('data', chunk => {
        for (const {kind, body} of frames.push(chunk)) {
            const msg = decode(kind, body)
            if (!msg) continue
            if (msg.kind === 'file') onFile(msg)
            else if (msg.kind === 'answer' || msg.kind === 'fs' || msg.kind === 'texture' || msg.kind === 'objects') {
                const done = pendingQueries.get(msg.request)
                pendingQueries.delete(msg.request)
                if (done) done(msg)
            } else if (msg.kind === 'legend') registerTokens(msg, context)
        }
    })
}

/**
 * SLK tables: the text with highlighting, or a table to edit, switchable
 * (`wc3.slk.openTable` / `wc3.slk.openText`, or Reopen Editor With).
 * The table is a custom text editor: the text stays the document, so undo,
 * save and dirty state are the text's. The server reads the sheet from the
 * text (`QUERY.SLK_TABLE`) and turns a written cell into text edits
 * (`QUERY.SLK_SET_CELL`), which are applied here as a workspace edit.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerSlk(context) {
    const media = Uri.joinPath(context.extensionUri, 'webview')
    const html = webview => {
        const nonce = [...Array(32)].map(() => Math.floor(Math.random() * 36).toString(36)).join('')
        const asset = name => webview.asWebviewUri(Uri.joinPath(media, name))
        return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource}; script-src 'nonce-${nonce}'">
<link rel="stylesheet" href="${asset('slk.css')}">
</head><body>
<div class="bar">
  <input id="search" type="search" placeholder="Find rows" title="Show only the rows with this text">
  <label class="check" title="Show row 1, the field names, as the header"><input id="header" type="checkbox">Row 1 as header</label>
  <label class="check" title="Keep column A in view"><input id="freeze" type="checkbox">Freeze column A</label>
  <div class="spacer"></div>
  <span id="problems" class="problems" hidden></span>
  <span id="info" class="info"></span>
  <button id="open-text" title="Open the text of this file">Open as Text</button>
</div>
<div id="area" class="area">
  <canvas id="canvas"></canvas>
  <div id="scroll" class="scroll" tabindex="0"><div id="sizer"></div></div>
  <input id="editor" hidden>
  <div id="tooltip" class="tooltip" hidden></div>
  <div id="loading" class="message">Reading the table…</div>
</div>
<script nonce="${nonce}" src="${asset('slk.js')}"></script>
</body></html>`
    }

    const editor = {
        resolveCustomTextEditor(document, panel) {
            const {webview} = panel
            webview.options = {enableScripts: true, localResourceRoots: [media]}
            webview.html = html(webview)
            const id = idOf(document)

            const post = async () => {
                const answer = await query(QUERY.SLK_TABLE, id, {line: 0, character: 0})
                if (answer && answer.text) webview.postMessage({type: 'table', table: JSON.parse(answer.text)})
            }
            let timer
            const refresh = () => {
                clearTimeout(timer)
                timer = setTimeout(post, 50)
            }
            const setCell = async (row, col, text) => {
                const answer = await query(QUERY.SLK_SET_CELL, id, {line: row, character: col}, text)
                if (!answer || !answer.text) {
                    window.showErrorMessage('WC3: the language server did not answer; the cell is unchanged.')
                    return refresh()
                }
                const changes = JSON.parse(answer.text)
                if (!changes.length) return refresh()
                const edit = new WorkspaceEdit()
                for (const [l, c, el, ec, t] of changes) edit.replace(document.uri, new Range(l, c, el, ec), t)
                if (!(await workspace.applyEdit(edit))) refresh()
            }
            const subscriptions = [
                workspace.onDidChangeTextDocument(e => {
                    if (e.document.uri.toString() === document.uri.toString()) refresh()
                }),
                webview.onDidReceiveMessage(msg => {
                    if (msg.type === 'ready') post()
                    else if (msg.type === 'set') setCell(msg.row, msg.col, msg.text).catch(e => window.showErrorMessage(`WC3: ${e.message}`))
                    else if (msg.type === 'openText') commands.executeCommand('vscode.openWith', document.uri, 'default', panel.viewColumn)
                }),
            ]
            panel.onDidDispose(() => {
                clearTimeout(timer)
                subscriptions.forEach(d => d.dispose())
            })
        },
    }

    /** The `.slk` a command is about: the one it was run on, or the active one. */
    const target = uri => {
        if (uri instanceof Uri) return uri
        const tab = window.tabGroups.activeTabGroup.activeTab
        const input = tab && tab.input
        return input && input.uri instanceof Uri ? input.uri : window.activeTextEditor?.document.uri
    }
    context.subscriptions.push(
        window.registerCustomEditorProvider('wc3.slkTable', editor, {
            supportsMultipleEditorsPerDocument: true,
            webviewOptions: {retainContextWhenHidden: true},
        }),
        commands.registerCommand('wc3.slk.openTable', uri => {
            const file = target(uri)
            if (file) commands.executeCommand('vscode.openWith', file, 'wc3.slkTable')
        }),
        commands.registerCommand('wc3.slk.openText', uri => {
            const file = target(uri)
            if (file) commands.executeCommand('vscode.openWith', file, 'default')
        }),
    )
}

/**
 * A trigger of its own (`.trg`): the text, or the trigger as World Editor
 * shows it (`wc3.trg.openVisual` / `wc3.trg.openText`, or Reopen Editor
 * With). The view is a custom text editor, as the SLK table: the text
 * stays the document, so undo, save and dirty state are the text's. The
 * server reads the view from the text (`QUERY.TRIGGER_VIEW`: phrases from
 * the project's `UI\TriggerData.txt` and `UI\TriggerStrings.txt`) and
 * turns what's done in it into text edits (`QUERY.TRIGGER_EDIT`), applied
 * here as a workspace edit. Copy and paste go through the clipboard as
 * lines of the text, so they paste into a text editor too.
 *
 * A map's triggers (`.wtg`) open the same way (`wc3.wtgTriggers`, over
 * their text under the `wc3obj:` scheme): a tree of the categories,
 * triggers and variables beside it (`webview/triggers.js` with `wtg`), and the trigger
 * picked there as a `.trg` shows. The server cuts that trigger out of the
 * text (`triggers::wtg_view`): the page speaks in the lines of its
 * section, which trigger it is (from 1, its place among them) goes before
 * the text of each query, and the edits come back as edits of the whole
 * text. The tree's operations (a new trigger or category, renaming or
 * deleting one, the variables) are queries of the same kind
 * (`triggers::map_op`); the answer's `pick` is the trigger to pick after
 * one. Only World Editor's own is offered there: the variables are
 * globals, and there's no Show JASS.
 *
 * @param {import('vscode').ExtensionContext} context
 */
/**
 * A map's triggers open: which trigger each picks (by the URI of its
 * text), for its Show JASS; and what tells that JASS it changed
 * (`registerTriggerJass` sets it).
 * @type {Map<string, number>}
 */
const wtgPicked = new Map()

/**
 * The trigger editors open, by their text's URI: what the tests in the
 * editor (`e2e/`) drive them with, as the page would.
 * @type {Map<string, {receive: (msg: any) => Promise<void>, state: () => any, wct: () => any, refresh: () => Promise<void>}>}
 */
const triggerEditors = new Map()
let onWtgPicked = (/** @type {import('vscode').Uri} */ _uri) => {}

/** The file named `name` beside `uri`, in its scheme. @param {import('vscode').Uri} uri @param {string} name */
function sibling(uri, name) {
    return uri.with({path: path.posix.join(path.posix.dirname(uri.path), name)})
}

/** A map's triggers and its custom script, each the other's (by file name). */
const PAIRED = {'war3map.wtg': 'war3map.wct', 'war3map.wct': 'war3map.wtg'}

/** A view's spans as their words (`triggers.js`'s `Span`: text, or a part with its own spans). */
function spanText(spans) {
    return (spans ?? []).map(s => (typeof s === 'string' ? s : spanText(s.s))).join('')
}

/**
 * What in a map's triggers names `name` (`gg_rct_Spawn`, `gg_unit_hfoo_0012`
 * ...): each line of `war3map.wtg`'s text that has it (`$name`), its
 * trigger and the line as World Editor's phrase (the server's view of its
 * trigger, `QUERY.TRIGGER_VIEW`, its line found by `at`), and each line
 * of the custom script beside it (`war3map.wct`) that has it, by the
 * trigger whose code it's in (`wct.code`) or the map's header (`nth` 0).
 *
 * @param {import('vscode').TextDocument} document  the map's `war3map.wtg` as text
 * @param {string} name
 * @returns {Promise<{nth: number, trigger: string, line: number, text: string, custom: boolean}[]>}
 */
async function usesOf(document, name) {
    const escaped = name.replace(/[^A-Za-z0-9_]/g, c => `\\${c}`)
    const inWtg = new RegExp(`\\$${escaped}(?![A-Za-z0-9_])`)
    const inCode = new RegExp(`(?<![A-Za-z0-9_])${escaped}(?![A-Za-z0-9_])`)
    // The wtg's lines naming it, by trigger (its place, from 1).
    const found = []
    let nth = 0
    for (const [i, line] of document.getText().split('\n').entries()) {
        if (/^\[trigger\]/.test(line)) nth++
        else if (nth && inWtg.test(line)) found.push({nth, line: i + 1, raw: line.slice(line.indexOf('=') + 1).trim()})
    }
    const id = idOf(document)
    const views = new Map()
    const viewOf = async n => {
        if (!views.has(n)) {
            const answer = await query(QUERY.TRIGGER_VIEW, id, {line: n, character: 0})
            views.set(n, answer?.text ? JSON.parse(answer.text) : null)
        }
        return views.get(n)
    }
    const uses = []
    for (const f of found) {
        const map = await viewOf(f.nth)
        // Its line in the trigger's view, and the node on it.
        const v = (map?.at ?? []).indexOf(f.line) + 1
        let phrase = ''
        const walk = nodes => {
            for (const n of nodes ?? []) {
                if (n.line === v) phrase = spanText(n.text)
                for (const b of n.blocks ?? []) walk(b.nodes)
            }
        }
        for (const list of [map?.view?.events, map?.view?.conditions, map?.view?.actions]) walk(list)
        uses.push({nth: f.nth, trigger: map?.triggers[f.nth - 1]?.name ?? '', line: f.line, text: phrase || f.raw, custom: false})
    }
    // The custom script's lines naming it.
    const wct = await workspace.openTextDocument(sibling(document.uri, 'war3map.wct')).then(d => d, () => null)
    if (wct) {
        const map = await viewOf(1)
        const code = map?.wct?.code ?? []
        const header = map?.wct?.header
        for (const [j, line] of wct.getText().split('\n').entries()) {
            if (!inCode.test(line)) continue
            const k = code.findIndex(c => c && c[0] < j && j < c[1])
            if (k >= 0) uses.push({nth: k + 1, trigger: map.triggers[k]?.name ?? '', line: j + 1, text: line.trim(), custom: true})
            else if (header && header[0] < j && j < header[1]) uses.push({nth: 0, trigger: '', line: j + 1, text: line.trim(), custom: true})
        }
    }
    return uses
}

function registerTrigger(context) {
    // A map's triggers and its custom script are saved together: the code
    // of its triggers of custom text is in the one by their places in the
    // other, so one without the other would put code to the wrong trigger.
    context.subscriptions.push(workspace.onDidSaveTextDocument(doc => {
        const other = PAIRED[path.posix.basename(doc.uri.path).toLowerCase()]
        if (!other || doc.uri.scheme !== OBJECTS_SCHEME) return
        const twin = sibling(doc.uri, other).toString().toLowerCase()
        const open = workspace.textDocuments.find(d => d.isDirty && d.uri.toString().toLowerCase() === twin)
        if (open) open.save()
    }))
    const media = Uri.joinPath(context.extensionUri, 'webview')
    /** The page; `wtg`: a map's triggers, with their tree beside it. */
    const html = (webview, wtg) => {
        const nonce = [...Array(32)].map(() => Math.floor(Math.random() * 36).toString(36)).join('')
        const asset = name => webview.asWebviewUri(Uri.joinPath(media, name))
        return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; font-src ${webview.cspSource}; img-src data:; script-src 'nonce-${nonce}'">
${triggerHead(name => String(asset(name)))}
</head><body>
<div id="trg-root" class="trg-root${wtg ? ' wtg' : ''}">
${triggerMarkup(wtg, triggerLangs())}
</div>
<script nonce="${nonce}" src="${asset('triggers.js')}"></script>
<script nonce="${nonce}">
// This tab's: the editor on its root, the page's API its host.
const page = TriggerEditor.mount(document.getElementById('trg-root'), acquireVsCodeApi())
window.addEventListener('message', e => page.receive(e.data))
</script>
</body></html>`
    }

    const editor = {
        async resolveCustomTextEditor(document, panel) {
            const {webview} = panel
            const wtg = /\.wtg$/i.test(document.uri.path)
            if (wtg && document.uri.scheme !== OBJECTS_SCHEME) {
                // The file itself: its text is what opens.
                await commands.executeCommand('vscode.openWith', Uri.file(document.uri.fsPath).with({scheme: OBJECTS_SCHEME}), 'wc3.wtgTriggers', panel.viewColumn)
                return panel.dispose()
            }
            webview.options = {enableScripts: true, localResourceRoots: [media]}
            webview.html = html(webview, wtg)
            const id = idOf(document)
            // A map's custom script beside its triggers (`war3map.wct`): the
            // code of its triggers of custom text, by their places. Open as
            // a text document, so what's edited and not saved counts.
            const wct = wtg ? await workspace.openTextDocument(sibling(document.uri, 'war3map.wct')).then(d => d, () => null) : null
            /** The last tree, for the code of its triggers. */
            let last = null
            let warned = false
            /** A map's triggers: which is picked (from 1; 0: the first), and the text's line of each of its lines. */
            let picked = 0
            /** The last parameter's dialog's choices (for the tests in the editor). */
            let lastChoices = null
            /** What the page said of the line beside the tree it was asked to drag (the tests). */
            let lastSplitter = null
            /** @type {number[]} */
            let at = []
            /** A query's text: in a map's triggers, after which is picked. */
            const of = text => wtg ? `${picked}\n${text}` : text

            const post = async select => {
                const answer = await query(QUERY.TRIGGER_VIEW, id, {line: picked, character: 0})
                if (!answer || !answer.text) return
                if (!wtg) return webview.postMessage({type: 'view', view: JSON.parse(answer.text), select, file: path.basename(document.uri.path).replace(/\.trg$/i, '')})
                const map = JSON.parse(answer.text)
                last = map
                picked = map.picked
                at = map.at
                if (map.wct?.problem && !warned) {
                    warned = true
                    window.showWarningMessage(`WC3: ${map.wct.problem}.`)
                }
                if (wtgPicked.get(document.uri.toString()) !== picked) {
                    wtgPicked.set(document.uri.toString(), picked)
                    onWtgPicked(document.uri)
                }
                webview.postMessage({type: 'tree', tree: map})
                if (map.view) webview.postMessage({type: 'view', view: map.view, select, file: ''})
            }
            let timer
            let select = 0
            const refresh = () => {
                clearTimeout(timer)
                timer = setTimeout(() => {
                    post(select)
                    select = 0
                }, 50)
            }
            /** One operation of `triggers::op` on the ECA at `line`. */
            const run = async (op, line, arg = '') => {
                const answer = await query(QUERY.TRIGGER_EDIT, id, {line, character: op}, of(arg))
                if (!answer || !answer.text) {
                    window.showErrorMessage('WC3: the language server did not answer; the trigger is unchanged.')
                    return null
                }
                const result = JSON.parse(answer.text)
                if (result.error) {
                    window.showWarningMessage(`WC3: ${result.error}`)
                    return null
                }
                if (result.edits) {
                    // A map's: the trigger to pick after it.
                    if (wtg && result.pick !== undefined) picked = result.pick
                    if (!result.edits.length && !result.files?.length && !result.wct?.length) {
                        if (result.line) webview.postMessage({type: 'select', line: result.line})
                        return result
                    }
                    const edit = new WorkspaceEdit()
                    for (const [l, c, el, ec, t] of result.edits) edit.replace(document.uri, new Range(l, c, el, ec), t)
                    // The custom script's pieces, moved with the triggers: one edit of both.
                    if (wct) for (const [l, c, el, ec, t] of result.wct ?? []) edit.replace(wct.uri, new Range(l, c, el, ec), t)
                    if (result.warning) window.showWarningMessage(`WC3: ${result.warning}`)
                    // A variable renamed: its uses in other files too.
                    for (const [file, l, c, el, ec, t] of result.files ?? []) edit.replace(uriOf(file), new Range(l, c, el, ec), t)
                    // A file made with it (a trigger's code beside it): one edit, one undo.
                    if (result.create) edit.createFile(Uri.file(result.create[0]), {contents: Buffer.from(result.create[1], 'utf8'), overwrite: false})
                    select = result.line || 0
                    if (!(await (result.wct ? applyPaired(edit) : workspace.applyEdit(edit)))) refresh()
                }
                return result
            }
            const OP = {TOGGLE: 0, DELETE: 1, MOVE: 2, PASTE: 3, COPY: 4, FIELD: 5, SET_PARAM: 6, NEW: 7, NEW_VARIABLE: 8, REPLACE: 9, SET_VARIABLE: 10, CHECK_VARIABLE: 11, RENAMED: 12,
                // A map's tree (`triggers::map_op`).
                NEW_TRIGGER: 20, NEW_CATEGORY: 21, DELETE_TRIGGER: 22, DELETE_CATEGORY: 23, RENAME_CATEGORY: 24, DELETE_VARIABLE: 25, MOVE_TRIGGER: 26,
                // Convert to Custom Text (`triggers::convert_trg`, `convert_wtg`).
                CONVERT: 27}
            /**
             * Shows the code of the `nth` trigger (from 1; 0: the map's
             * custom script) in `war3map.wct`, beside: its text editor, so
             * the code is edited as the JASS it is.
             */
            const openCode = async (nth, preserveFocus = false) => {
                // A trigger file's: the `.j` of its name beside it.
                if (!wtg) {
                    const code = document.uri.with({path: document.uri.path.replace(/\.trg$/i, '.j')})
                    const there = await workspace.fs.stat(code).then(() => true, () => false)
                    if (!there) return window.showWarningMessage(`WC3: no ${path.basename(code.path)} beside the trigger: its code is still the map's (war3map.wct); Extract Triggers again to have it beside.`)
                    return window.showTextDocument(code, {viewColumn: ViewColumn.Beside, preserveFocus, preview: true})
                }
                if (!wct) return window.showWarningMessage('WC3: the map has no custom script (war3map.wct) beside its triggers.')
                const piece = nth === 0 ? last?.wct?.header : last?.wct?.code[nth - 1]
                if (!piece) return window.showWarningMessage(`WC3: war3map.wct has no code of ${nth === 0 ? 'the map' : 'this trigger'}.`)
                // Its code alone, as a document of its own: the others' out of reach.
                const name = nth === 0 ? null : last.triggers[nth - 1].name
                const doc = await workspace.openTextDocument(pieceUri(wct.uri, name))
                const shown = window.visibleTextEditors.find(e => e.document.uri.scheme === PIECE_SCHEME && pieceScript(e.document.uri).toString() === wct.uri.toString())
                await window.showTextDocument(doc, {viewColumn: shown?.viewColumn ?? ViewColumn.Beside, preserveFocus, preview: true})
            }
            /** A piece of the trigger named `old` open: reopened as its new name's. */
            const followRename = async old => {
                const was = pieceUri(wct.uri, old).toString()
                const tab = window.tabGroups.all.flatMap(g => g.tabs).find(t => t.input?.uri?.toString() === was)
                if (!tab) return
                await post(0)
                const name = last?.triggers[picked - 1]?.name
                if (name === undefined || name === old) return
                const doc = await workspace.openTextDocument(pieceUri(wct.uri, name))
                await window.showTextDocument(doc, {viewColumn: tab.group.viewColumn, preserveFocus: true, preview: false})
                const open = workspace.textDocuments.find(d => d.uri.toString() === was)
                if (open?.isDirty) await open.save()
                await window.tabGroups.close(tab, true)
            }
            /** Shows the text at `line`: of the picked trigger's section, or (`own`) the text's. */
            const reveal = async (line, own = false) => {
                if (wtg && !own) line = at[line - 1] ?? at[0] ?? 1
                const where = new Position(Math.max(0, line - 1), 0)
                const shown = window.visibleTextEditors.find(e => e.document === document)
                const ed = shown ?? await window.showTextDocument(document, {viewColumn: ViewColumn.Beside, preserveFocus: false})
                ed.selection = new Selection(where, where)
                ed.revealRange(new Range(where, where), TextEditorRevealType.InCenterIfOutsideViewport)
                if (shown) window.showTextDocument(document, shown.viewColumn)
            }
            /**
             * Changes or deletes (`arg` empty) the variable at `line`: what's
             * safe (its uses renamed with it) at once; what breaks uses
             * (deleting a used one, a narrower scope, another type, array or
             * not, its name in custom script) only once the places are shown
             * and it's confirmed, and they're offered to go to after.
             */
            const changeVariable = async (line, arg) => {
                const answer = await query(QUERY.TRIGGER_EDIT, id, {line, character: OP.CHECK_VARIABLE}, arg)
                if (!answer || !answer.text) return window.showErrorMessage('WC3: the language server did not answer; the trigger is unchanged.')
                const check = JSON.parse(answer.text)
                if (check.error) return window.showWarningMessage(`WC3: ${check.error}`)
                const places = check.places ?? []
                const deleting = !arg
                if (places.length) {
                    const what = deleting ? 'Deleting the variable' : 'This change'
                    const shown = places.slice(0, 12).map(([file, l, why]) => `${path.basename(file || document.uri.path)}:${l}: ${why}`)
                    if (places.length > shown.length) shown.push(`… and ${places.length - shown.length} more`)
                    const go = deleting ? 'Delete Anyway' : 'Change Anyway'
                    const picked = await window.showWarningMessage(
                        `${what} breaks ${places.length} place${places.length === 1 ? '' : 's'} that will need fixing by hand.`,
                        {modal: true, detail: shown.join('\n')}, go)
                    if (picked !== go) return
                }
                const done = deleting ? await run(wtg ? OP.DELETE_VARIABLE : OP.DELETE, line) : await run(OP.SET_VARIABLE, line, arg)
                if (!done) return
                if (check.renamed) window.setStatusBarMessage(`WC3: ${check.renamed} use${check.renamed === 1 ? '' : 's'} of the variable renamed`, 5000)
                if (places.length) showPlaces(places)
            }
            /** The places a change broke, to go to one by one. */
            const showPlaces = async places => {
                const show = 'Show Places'
                if (await window.showWarningMessage(`WC3: ${places.length} place${places.length === 1 ? '' : 's'} to fix.`, show) !== show) return
                // A file of `''`: this text's line (a map's triggers).
                const name = file => path.basename(file || document.uri.path)
                const items = places.map(([file, l, why]) => ({label: `${name(file)}:${l}`, description: why, detail: file || document.uri.fsPath, file, line: l}))
                for (;;) {
                    const item = await window.showQuickPick(items, {title: 'Places to fix', matchOnDescription: true, ignoreFocusOut: true})
                    if (!item) return
                    const where = new Position(Math.max(0, item.line - 1), 0)
                    if (item.file) await window.showTextDocument(uriOf(item.file), {selection: new Range(where, where), preview: false})
                    else await reveal(item.line, true)
                    item.label = `$(check) ${name(item.file)}:${item.line}`
                }
            }
            const handle = async msg => {
                switch (msg.type) {
                // What the page answered the tests in the editor.
                case 'e2eSplitter': lastSplitter = msg; return
                case 'ready': {
                    // The editor's icons once (the project's), then the view.
                    const icons = await query(QUERY.TRIGGER_ICONS, id, {line: 0, character: 0}, '')
                    if (icons && icons.text) webview.postMessage({type: 'icons', icons: JSON.parse(icons.text)})
                    return post(0)
                }
                case 'toggle': return run(OP.TOGGLE, msg.line, msg.lines || '')
                case 'delete': return run(OP.DELETE, msg.line, msg.lines || '')
                case 'move': return run(OP.MOVE, msg.line, msg.lines ? `${msg.place}\n${msg.lines}` : msg.place)
                case 'field': return run(OP.FIELD, 0, `${msg.key}\n${msg.value}`)
                case 'renamed': {
                    const done = await run(OP.RENAMED, 0, msg.from)
                    // Its code open: under its new name, where it was.
                    if (done && wct) await followRename(msg.from)
                    return done
                }
                case 'copy':
                case 'cut': {
                    const copied = await run(OP.COPY, msg.line, msg.lines || '')
                    if (!copied || copied.text === undefined) return
                    await env.clipboard.writeText(copied.text)
                    if (msg.type === 'cut') await run(OP.DELETE, msg.line, msg.lines || '')
                    return
                }
                case 'paste': {
                    const text = await env.clipboard.readText()
                    if (!text.trim()) return window.showWarningMessage('WC3: the clipboard is empty.')
                    return run(OP.PASTE, 0, `${msg.place}\n${text}`)
                }
                case 'choices': {
                    const answer = await query(QUERY.TRIGGER_CHOICES, id, {line: msg.line, character: 0}, of(msg.path))
                    if (!answer || !answer.text) return
                    const choices = JSON.parse(answer.text)
                    if (choices.error) return window.showWarningMessage(`WC3: ${choices.error}`)
                    lastChoices = choices
                    return webview.postMessage({type: 'choices', line: msg.line, path: msg.path, choices})
                }
                case 'set': return run(OP.SET_PARAM, msg.line, `${msg.path}\n${msg.kind}\n${msg.value}`)
                case 'catalog': {
                    const answer = await query(QUERY.TRIGGER_CATALOG, id, {line: 0, character: msg.kind}, '')
                    if (answer && answer.text) webview.postMessage({type: 'catalog', kind: msg.kind, place: msg.place, line: msg.line || 0, items: JSON.parse(answer.text)})
                    return
                }
                case 'new': return run(OP.NEW, 0, `${msg.place}\n${msg.kind} ${msg.name}`)
                case 'types': {
                    const answer = await query(QUERY.TRIGGER_CATALOG, id, {line: 0, character: 3}, '')
                    if (answer && answer.text) webview.postMessage({type: 'types', line: msg.line || 0, items: JSON.parse(answer.text)})
                    return
                }
                case 'newVariable':
                case 'setVariable': {
                    const arg = [msg.name, msg.vtype, msg.array ? '1' : '0', msg.size, msg.scope, msg.initial].join('\n')
                    return msg.type === 'setVariable' ? changeVariable(msg.line, arg) : run(OP.NEW_VARIABLE, 0, arg)
                }
                case 'deleteVariable': return changeVariable(msg.line, '')
                case 'replace': return run(OP.REPLACE, msg.line, msg.name)
                case 'reveal': return reveal(msg.line)
                case 'revealText': return reveal(msg.line, true)
                case 'pick':
                    picked = msg.nth
                    await post(0)
                    // Custom text: its code, beside.
                    if (last?.triggers[picked - 1]?.custom) await openCode(picked, true)
                    return
                case 'openCode': return openCode(msg.picked ? picked : msg.nth)
                case 'convert': {
                    const name = wtg ? last?.triggers[picked - 1]?.name : path.basename(document.uri.path, '.trg')
                    const go = msg.confirmed ? 'Convert' : await window.showWarningMessage(
                        `Convert ${name ?? 'the trigger'} to custom text? Its events, conditions and actions become its JASS, ${wtg ? "in the map's custom script (war3map.wct)" : 'in the .j file beside it'}.`, {modal: true}, 'Convert')
                    if (go !== 'Convert') return
                    const result = await run(OP.CONVERT, 0)
                    // Its code shown, as World Editor shows it after converting.
                    if (result && !result.error) {
                        await post(0)
                        return openCode(wtg ? picked : 0, true)
                    }
                    return
                }
                case 'extract': return commands.executeCommand('wc3.wtg.extract', document.uri)
                case 'newCategory': return run(OP.NEW_CATEGORY, 0, msg.name)
                case 'newTrigger': return run(OP.NEW_TRIGGER, 0, `${msg.category}\n${msg.name}`)
                case 'renameCategory': return run(OP.RENAME_CATEGORY, 0, `${msg.index}\n${msg.name}`)
                case 'moveTrigger': return run(OP.MOVE_TRIGGER, 0, `${msg.moved}\n${msg.category}\n${msg.before}`)
                case 'deleteTrigger': {
                    const go = await window.showWarningMessage(`Delete the trigger ${msg.name}?`, {modal: true}, 'Delete')
                    return go === 'Delete' ? run(OP.DELETE_TRIGGER, 0) : undefined
                }
                case 'deleteCategory': {
                    const what = msg.count ? ` and its ${msg.count} trigger${msg.count === 1 ? '' : 's'}` : ''
                    const go = await window.showWarningMessage(`Delete the category ${msg.name}${what}?`, {modal: true}, 'Delete')
                    return go === 'Delete' ? run(OP.DELETE_CATEGORY, 0, String(msg.index)) : undefined
                }
                case 'openText':
                    // A map's: its text, which `default` would open here again.
                    if (wtg) return window.showTextDocument(document, {viewColumn: panel.viewColumn, preview: false})
                    return commands.executeCommand('vscode.openWith', document.uri, 'default', panel.viewColumn)
                case 'show': {
                    const shown = SHOWN.find(s => s.id === msg.language)
                    return shown && commands.executeCommand(shown.command, document.uri)
                }
                }
            }
            let queue = Promise.resolve()
            /** A message as the page sends it, in its turn. */
            const receive = msg => (queue = queue.then(() => handle(msg)).catch(e => window.showErrorMessage(`WC3: ${e.message}`)))
            const subscriptions = [
                workspace.onDidChangeTextDocument(e => {
                    if (e.document.uri.toString() === document.uri.toString() || (wct && e.document === wct)) refresh()
                }),
                // One at a time: each edit is worked out on the text the one
                // before left (typing a name sends one after another).
                webview.onDidReceiveMessage(receive),
            ]
            // For the tests in the editor (`e2e/`): what the page would send, and what it was last sent.
            const key = document.uri.toString()
            triggerEditors.set(key, {receive, state: () => ({picked, tree: last, choices: lastChoices, splitter: lastSplitter}), wct: () => wct, refresh: () => post(0), page: m => webview.postMessage(m)})
            panel.onDidDispose(() => {
                clearTimeout(timer)
                subscriptions.forEach(d => d.dispose())
                triggerEditors.delete(key)
            })
        },
    }

    /** The `.trg` a command is about: the one it was run on, or the active one. */
    const target = uri => {
        if (uri instanceof Uri) return uri
        const tab = window.tabGroups.activeTabGroup.activeTab
        const input = tab && tab.input
        return input && input.uri instanceof Uri ? input.uri : window.activeTextEditor?.document.uri
    }
    context.subscriptions.push(
        window.registerCustomEditorProvider('wc3.trigger', editor, {
            supportsMultipleEditorsPerDocument: true,
            webviewOptions: {retainContextWhenHidden: true},
        }),
        window.registerCustomEditorProvider('wc3.wtgTriggers', editor, {
            supportsMultipleEditorsPerDocument: true,
            webviewOptions: {retainContextWhenHidden: true},
        }),
        commands.registerCommand('wc3.wtg.openVisual', uri => {
            let file = target(uri)
            if (!file) return
            // From the custom script: the triggers beside it.
            if (/\.wct$/i.test(file.path)) file = sibling(file, 'war3map.wtg')
            commands.executeCommand('vscode.openWith', Uri.file(file.fsPath).with({scheme: OBJECTS_SCHEME}), 'wc3.wtgTriggers')
        }),
        commands.registerCommand('wc3.trg.openVisual', uri => {
            const file = target(uri)
            if (file) commands.executeCommand('vscode.openWith', file, 'wc3.trigger')
        }),
        commands.registerCommand('wc3.trg.openText', async uri => {
            const file = target(uri)
            if (!file) return
            // A map's triggers: their text, which `default` would open as triggers again.
            if (/\.wtg$/i.test(file.path)) return window.showTextDocument(await workspace.openTextDocument(Uri.file(file.fsPath).with({scheme: OBJECTS_SCHEME})), {preview: false})
            commands.executeCommand('vscode.openWith', file, 'default')
        }),
    )
    // Another page's place for the same trigger editor (the World
    // Editor's Triggers panel): `panel` its webview's stand-in
    // (`postMessage`, `onDidReceiveMessage` ...), the page has the markup.
    return {host: (document, panel) => editor.resolveCustomTextEditor(document, panel), langs: triggerLangs, usesOf}
}

/**
 * Extract Triggers (`wc3.wtg.extract`, on the text of a `.wtg` and on the
 * file in the Explorer): every trigger as a `.trg` of its own in a folder
 * picked for it, a folder per category, the variables in `_variables.trg`.
 * The server splits the text (`QUERY.TRIGGER_EXTRACT`, so what's edited
 * and not saved counts); the files are written here, through VS Code, so
 * a folder inside a map works as any other. Files already there are
 * replaced, after asking.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerTriggerExtract(context) {
    const run = async uri => {
        let doc
        if (uri instanceof Uri) doc = await workspace.openTextDocument(Uri.file(uri.fsPath).with({scheme: OBJECTS_SCHEME}))
        else doc = window.activeTextEditor?.document
        if (!doc || doc.languageId !== 'wtg') return window.showWarningMessage('WC3: open the triggers (war3map.wtg) first.')
        const answer = await query(QUERY.TRIGGER_EXTRACT, idOf(doc), {line: 0, character: 0}, '')
        if (!answer || !answer.text) return window.showErrorMessage('WC3: the language server did not answer.')
        const result = JSON.parse(answer.text)
        if (result.error) return window.showErrorMessage(`WC3: ${result.error}`)
        const onDisk = doc.uri.fsPath && fs.existsSync(doc.uri.fsPath) ? Uri.file(path.dirname(doc.uri.fsPath)) : undefined
        const picked = await window.showOpenDialog({
            canSelectFiles: false, canSelectFolders: true, canSelectMany: false, defaultUri: onDisk,
            openLabel: 'Extract Here', title: `Extract ${result.files.length} files of ${path.basename(doc.uri.fsPath)}`,
        })
        if (!picked || !picked.length) return
        const root = picked[0]
        const at = rel => Uri.joinPath(root, ...rel.split('/'))
        const exists = async u => workspace.fs.stat(u).then(() => true, () => false)
        const taken = []
        for (const [rel] of result.files) if (await exists(at(rel))) taken.push(rel)
        if (taken.length) {
            const go = await window.showWarningMessage(
                `${taken.length} of the files are there already (${taken.slice(0, 3).join(', ')}${taken.length > 3 ? ', …' : ''}). Replace them?`,
                {modal: true}, 'Replace')
            if (go !== 'Replace') return
        }
        for (const folder of result.folders) await workspace.fs.createDirectory(at(folder))
        for (const [rel, text] of result.files) await workspace.fs.writeFile(at(rel), Buffer.from(text, 'utf8'))
        const triggers = result.files.filter(([rel]) => rel.endsWith('.trg') && !rel.endsWith('_variables.trg')).length
        const code = result.files.filter(([rel]) => rel.endsWith('.j')).length
        const reveal = await window.showInformationMessage(
            `WC3: ${triggers} triggers in ${result.folders.length} folders extracted to ${root.fsPath}${code ? `, with the code of ${code} of custom text beside them (.j)` : ''}.`, 'Reveal')
        if (reveal) commands.executeCommand('revealInExplorer', root).then(undefined, () => commands.executeCommand('revealFileInOS', root))
    }
    context.subscriptions.push(commands.registerCommand('wc3.wtg.extract', uri => run(uri).catch(e => window.showErrorMessage(`WC3: ${e.message}`))))
}

/**
 * A language's icon (`icons/`), as the page writes it: the SVG itself
 * (the page's policy loads no images but `data:`), read once.
 * @type {Map<string, string>}
 */
const languageIcons = new Map()
/** @param {string} name */
/** The trigger editor's Show JASS / Lua / AngelScript buttons (`trigger-markup.js`). */
function triggerLangs() {
    return SHOWN.map(s => `<button id="show-${s.id}" class="lang-button" data-tip="Show ${s.name}: what this trigger is built into, beside it" aria-label="Show ${s.name}">${languageIcon(s.icon)}</button>`).join('')
}

function languageIcon(name) {
    if (!languageIcons.has(name)) {
        let svg = ''
        try {
            svg = fs.readFileSync(path.join(__dirname, 'icons', name), 'utf8').replace(/<\?xml[^>]*>/, '').trim()
        } catch { /* not shipped: the button keeps its label */ }
        languageIcons.set(name, svg.replace('<svg ', '<svg aria-hidden="true" focusable="false" '))
    }
    return languageIcons.get(name)
}

/**
 * Show JASS, Show Lua, Show AngelScript (`wc3.trg.showJass`,
 * `wc3.trg.showLua`, `wc3.trg.showAngelScript`; [`SHOWN`]): what a
 * `.trg` is built into, beside it, read-only, under each one's scheme
 * (`<name>.j`, `.lua`, `.as`, the trigger's URI in the query). A map's
 * triggers (`.wtg`) have them too (`war3map.wtg.j` ...), of the trigger
 * picked in their view, which follows the picking. The server builds them
 * from the text as it is (`QUERY.TRIGGER_JASS`, `wtg_rs::jass`; Lua and
 * AngelScript that JASS made each by `wc3-build`, `QUERY.TRIGGER_LUA`,
 * `QUERY.TRIGGER_ANGELSCRIPT`), and they follow the text as it changes,
 * saved or not. They aren't files of the project, but the server analyzes
 * each in its language without a file: its highlighting, its diagnostics.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerTriggerJass(context) {
    const changed = new EventEmitter()
    /** @param {typeof SHOWN[number]} shown @param {import('vscode').Uri} trg */
    const shownUri = (shown, trg) => Uri.from({
        scheme: shown.scheme,
        path: /\.wtg$/i.test(trg.path) ? `${trg.path}.${shown.ext}` : `${trg.path.replace(/\.trg$/i, '')}.${shown.ext}`,
        query: trg.toString(),
    })
    /** @type {Map<string, NodeJS.Timeout>} */
    const timers = new Map()
    /** What's shown of it, if it's open, is told it changed. @param {import('vscode').Uri} src */
    const refresh = src => {
        for (const shown of SHOWN) {
            const uri = shownUri(shown, src)
            if (!workspace.textDocuments.some(d => d.uri.toString() === uri.toString())) continue
            clearTimeout(timers.get(uri.toString()))
            timers.set(uri.toString(), setTimeout(() => changed.fire(uri), 150))
        }
    }
    onWtgPicked = refresh
    /** @param {typeof SHOWN[number]} shown */
    const provider = shown => ({
        onDidChange: changed.event,
        /** @param {import('vscode').Uri} uri */
        async provideTextDocumentContent(uri) {
            const trg = Uri.parse(uri.query)
            const doc = workspace.textDocuments.find(d => d.uri.toString() === trg.toString()) ?? await workspace.openTextDocument(trg)
            // A map's: the trigger its view picks.
            const which = /\.wtg$/i.test(trg.path) ? String(wtgPicked.get(trg.toString()) ?? 0) : ''
            const answer = await query(QUERY[shown.query], idOf(doc), {line: 0, character: 0}, which)
            const comment = shown.id === 'lua' ? '--' : '//'
            return answer ? answer.text : `${comment} The language server did not answer.\n`
        },
    })
    /** The `.trg` a command is about: the one it was run on, or the active one. */
    const target = uri => {
        if (uri instanceof Uri) return uri
        const tab = window.tabGroups.activeTabGroup.activeTab
        const input = tab && tab.input
        return input && input.uri instanceof Uri ? input.uri : window.activeTextEditor?.document.uri
    }
    for (const shown of SHOWN) {
        context.subscriptions.push(
            workspace.registerTextDocumentContentProvider(shown.scheme, provider(shown)),
            commands.registerCommand(shown.command, async uri => {
                let trg = target(uri)
                if (!trg || !/\.(trg|wtg)$/i.test(trg.path)) return window.showWarningMessage('WC3: open a trigger (.trg) or a map\'s triggers (war3map.wtg) first.')
                // A map's triggers: their text's.
                if (/\.wtg$/i.test(trg.path)) trg = Uri.file(trg.fsPath).with({scheme: OBJECTS_SCHEME})
                const doc = await workspace.openTextDocument(shownUri(shown, trg))
                await window.showTextDocument(doc, {viewColumn: ViewColumn.Beside, preview: true, preserveFocus: true})
                return doc
            }),
        )
    }
    context.subscriptions.push(
        changed,
        // The text is the one truth, saved or not (the view is another way
        // to edit it): what's shown follows it as it changes, whoever
        // changes it. After the server has the change: the text is sent as
        // it changes, and this waits.
        workspace.onDidChangeTextDocument(e => {
            if (/\.(trg|wtg)$/i.test(e.document.uri.path)) refresh(e.document.uri)
        }),
    )
}

/**
 * The txt2 preview, beside the text as Markdown's (`wc3.txt2.preview`,
 * `wc3.txt2.previewSide`): a page per document showing the server's HTML
 * for it (`QUERY.TXT2_HTML`), updated as the text changes. The page and
 * the editor follow each other's scrolling by the lines blocks start at;
 * a double click in the page moves the cursor there; links open in the
 * browser.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerTxt2Preview(context) {
    const media = Uri.joinPath(context.extensionUri, 'webview')
    /** @type {Map<string, {panel: import('vscode').WebviewPanel, doc: import('vscode').TextDocument, timer?: NodeJS.Timeout, revealing: number}>} */
    const previews = new Map()

    const page = webview => {
        const nonce = [...Array(32)].map(() => Math.floor(Math.random() * 36).toString(36)).join('')
        // FontAwesome 6 for `/иконка/...`: its classes, and its fonts,
        // which its stylesheet leaves to the page.
        const fa = name => webview.asWebviewUri(Uri.joinPath(media, 'fontawesome', name))
        return `<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; font-src ${webview.cspSource}; img-src https: http: data: ${webview.cspSource}; media-src https: http:; script-src 'nonce-${nonce}'">
<link rel="stylesheet" href="${fa('fontawesome.min.css')}">
<style>
@font-face { font-family: "Font Awesome 6 Free"; font-style: normal; font-weight: 900; font-display: block; src: url("${fa('fa-solid-900.woff2')}") format("woff2"); }
@font-face { font-family: "Font Awesome 6 Free"; font-style: normal; font-weight: 400; font-display: block; src: url("${fa('fa-regular-400.woff2')}") format("woff2"); }
</style>
</head><body class="t2"><div id="content"></div>
<script nonce="${nonce}" src="${webview.asWebviewUri(Uri.joinPath(media, 'txt2.js'))}"></script>
</body></html>`
    }

    const editorOf = doc => window.visibleTextEditors.find(e => e.document === doc)

    const render = async (key, line) => {
        const p = previews.get(key)
        if (!p) return
        const answer = await query(QUERY.TXT2_HTML, idOf(p.doc), {line: 0, character: 0})
        if (answer) p.panel.webview.postMessage({type: 'html', html: answer.text, line})
    }

    const open = (uri, column) => {
        const editor = window.activeTextEditor
        const doc = uri ? workspace.textDocuments.find(d => d.uri.toString() === uri.toString()) : editor?.document
        if (!doc || doc.languageId !== 'txt2') return
        const key = doc.uri.toString()
        const existing = previews.get(key)
        if (existing) return existing.panel.reveal(column, true)
        const panel = window.createWebviewPanel('wc3.txt2Preview', `Preview ${path.basename(doc.uri.path)}`,
            {viewColumn: column, preserveFocus: true},
            {enableScripts: true, localResourceRoots: [media], retainContextWhenHidden: true})
        panel.iconPath = Uri.joinPath(context.extensionUri, 'icons', 'txt2.svg')
        panel.webview.html = page(panel.webview)
        const entry = {panel, doc, revealing: 0}
        previews.set(key, entry)
        panel.onDidDispose(() => previews.delete(key))
        panel.webview.onDidReceiveMessage(async msg => {
            const ed = editorOf(doc)
            if (msg.type === 'ready') {
                render(key, ed ? ed.visibleRanges[0]?.start.line + 1 : undefined)
            } else if (msg.type === 'reveal' && ed) {
                entry.revealing = Date.now()
                const at = new Position(Math.max(0, msg.line - 1), 0)
                ed.revealRange(new Range(at, at), TextEditorRevealType.AtTop)
            } else if (msg.type === 'select') {
                const at = new Position(Math.max(0, msg.line - 1), 0)
                const target = ed ?? await window.showTextDocument(doc, {viewColumn: ViewColumn.One})
                target.selection = new Selection(at, at)
                target.revealRange(new Range(at, at), TextEditorRevealType.InCenterIfOutsideViewport)
                window.showTextDocument(target.document, target.viewColumn)
            } else if (msg.type === 'open' && msg.href) {
                try {
                    env.openExternal(Uri.parse(msg.href))
                } catch { /* not a link */ }
            }
        })
    }

    context.subscriptions.push(
        commands.registerCommand('wc3.txt2.preview', uri => open(uri instanceof Uri ? uri : undefined, ViewColumn.Active)),
        commands.registerCommand('wc3.txt2.previewSide', uri => open(uri instanceof Uri ? uri : undefined, ViewColumn.Beside)),
        // After the server has the change: the text is sent as it changes,
        // and this waits.
        workspace.onDidChangeTextDocument(e => {
            const key = e.document.uri.toString()
            const p = previews.get(key)
            if (!p) return
            clearTimeout(p.timer)
            p.timer = setTimeout(() => render(key), 120)
        }),
        window.onDidChangeTextEditorVisibleRanges(e => {
            const p = previews.get(e.textEditor.document.uri.toString())
            if (!p || Date.now() - p.revealing < 400) return
            const first = e.visibleRanges[0]
            if (first) p.panel.webview.postMessage({type: 'scroll', line: first.start.line + 1})
        }),
    )
}

/**
 * The `[build]`s of a `wc3.bni`'s text: each one's name and the line of
 * its `[build]`; `''` for one without a name, the default build.
 *
 * @param {string} text
 * @returns {{name: string, line: number}[]}
 */
function manifestBuilds(text) {
    const out = []
    let current = null
    text.split(/\r?\n/).forEach((raw, line) => {
        const t = raw.replace(/\/\/.*$/, '').trim()
        const section = /^\[([^\]]*)\]/.exec(t)
        if (section) {
            current = section[1].trim() === 'build' ? {line, name: ''} : null
            if (current) out.push(current)
            return
        }
        const name = /^name\s*=\s*(.*)$/.exec(t)
        if (current && name && current.name === '') current.name = name[1].trim().replace(/^"(.*)"$/, '$1')
    })
    return out
}

/**
 * The `wc3.bni` of a file or folder: the nearest in it or above it.
 *
 * @param {string} start
 * @returns {string | null}
 */
function manifestOf(start) {
    let dir = fs.existsSync(start) && fs.statSync(start).isDirectory() ? start : path.dirname(start)
    for (;;) {
        const candidate = path.join(dir, 'wc3.bni')
        if (fs.existsSync(candidate)) return candidate
        const up = path.dirname(dir)
        if (up === dir) return null
        dir = up
    }
}

/**
 * The `wc3.bni` a command is about: its argument's (a file's), the
 * active editor's project, the workspace's (asked, `title`, when there
 * are several).
 *
 * @param {unknown} arg
 * @param {string} title
 * @returns {Promise<string | null>}
 */
async function pickManifest(arg, title) {
    if (arg instanceof Uri && arg.scheme === 'file') return manifestOf(arg.fsPath)
    const active = window.activeTextEditor?.document.uri
    if (active?.scheme === 'file') {
        const m = manifestOf(active.fsPath)
        if (m) return m
    }
    const found = await workspace.findFiles('**/wc3.bni', '**/node_modules/**', 50)
    if (found.length <= 1) return found[0]?.fsPath ?? null
    const picked = await window.showQuickPick(found.map(u => ({label: workspace.asRelativePath(u), uri: u})), {title})
    return picked?.uri.fsPath ?? null
}

/**
 * Building a project's map (`wc3.build`): from the command palette (the
 * project of the active file, else of the workspace), the Explorer's
 * menu of a `wc3.bni`, the button over an open `wc3.bni`, and a lens over
 * each of its `[build]`s. The build runs in a process of its own (`wc3
 * build`, the binary the extension ships), so the language server goes
 * on answering; what it says goes to the output "WC3 Build". The
 * project's files open with changes are saved first: the build reads
 * the disk.
 *
 * @param {import('vscode').ExtensionContext} context
 * @param {string} binary
 */
function registerBuild(context, binary) {
    const output = window.createOutputChannel('WC3 Build')
    const setup = require('./test-setup').registerTestSetup(context)
    let running = false

    const run = async (arg, name) => {
        if (running) return window.showWarningMessage('WC3: a build is running.')
        const manifest = await pickManifest(arg, 'Build the map of which project?')
        if (!manifest) return window.showWarningMessage('WC3: no wc3.bni in the workspace or above the active file.')
        const root = path.dirname(manifest)
        const open = workspace.textDocuments.find(d => d.uri.scheme === 'file' && d.uri.fsPath === manifest)
        const builds = manifestBuilds(open ? open.getText() : fs.readFileSync(manifest, 'utf8'))
        // No name: the default build ([build] without one); with none and
        // several, which.
        if (name === undefined && builds.length > 1 && !builds.some(b => b.name === '')) {
            const picked = await window.showQuickPick(builds.map(b => ({label: b.name, name: b.name})), {title: `Build which of ${path.basename(root)}?`})
            if (!picked) return
            name = picked.name
        }
        // The build reads the disk: the project's files open with changes saved.
        for (const d of workspace.textDocuments) if (d.isDirty && d.uri.scheme === 'file' && d.uri.fsPath.startsWith(root + path.sep)) await d.save()

        running = true
        output.clear()
        let logged = ''
        const args = ['build', '--project', root, ...(name ? [name] : [])]
        const title = name || 'the default build'
        output.appendLine(`> wc3 ${args.join(' ')}`)
        const result = await window.withProgress(
            {location: ProgressLocation.Notification, title: `WC3: building ${title} of ${path.basename(root)}…`},
            () => new Promise(resolve => {
                const child = spawn(binary, args, {cwd: root})
                let out = ''
                child.stdout.on('data', d => { out += d; logged += d; output.append(d.toString()) })
                child.stderr.on('data', d => { logged += d; output.append(d.toString()) })
                child.on('error', e => resolve({code: -1, out: e.message}))
                child.on('close', code => resolve({code, out}))
            }),
        ).finally(() => { running = false })
        // Said, not waited for: the command is done when the build is.
        // 2: written, with errors in its script (the output has them).
        if (result.code === 2) {
            output.show(true)
            const errors = (logged.match(/^error: /gm) || []).length
            window.showErrorMessage(`WC3: built, with ${errors} error${errors === 1 ? '' : 's'} in its script; see the output "WC3 Build".`)
        } else if (result.code === 0) {
            const built = /^[^:]+: (.+?) \(/.exec(result.out.trim())
            const done = built ? built[1] : root
            window.showInformationMessage(`WC3: built ${done}`, 'Reveal', 'Show Output').then(go => {
                if (go === 'Reveal') commands.executeCommand('revealFileInOS', Uri.file(done))
                if (go === 'Show Output') output.show(true)
            })
        } else {
            output.show(true)
            window.showErrorMessage('WC3: the build failed; see the output "WC3 Build".')
        }
        return {code: result.code, output: logged}
    }

    // Test Map: the map built, copied into the game's folder, the game run
    // with it (`wc3 test`, `wc3_server::test_map`); what it says to the
    // same output.
    const test = async arg => {
        if (running) return window.showWarningMessage('WC3: a build is running.')
        const manifest = await pickManifest(arg, 'Test the map of which project?')
        if (!manifest) return window.showWarningMessage('WC3: no wc3.bni in the workspace or above the active file.')
        const root = path.dirname(manifest)
        for (const d of workspace.textDocuments) if (d.isDirty && d.uri.scheme === 'file' && d.uri.fsPath.startsWith(root + path.sep)) await d.save()
        running = true
        output.clear()
        const args = ['test', '--project', root]
        output.appendLine(`> wc3 ${args.join(' ')}`)
        const result = await window.withProgress(
            {location: ProgressLocation.Notification, title: `WC3: testing the map of ${path.basename(root)}…`},
            () => new Promise(resolve => {
                const child = spawn(binary, args, {cwd: root})
                let out = ''
                let err = ''
                child.stdout.on('data', d => { out += d; output.append(d.toString()) })
                child.stderr.on('data', d => { err += d; output.append(d.toString()) })
                child.on('error', e => resolve({code: -1, out: '', err: e.message}))
                child.on('close', code => resolve({code, out, err}))
            }),
        ).finally(() => { running = false })
        if (result.code === 0) window.showInformationMessage('WC3: the game started with the map.')
        else {
            output.show(true)
            const first = /^error: (.*)$/m.exec(result.err)?.[1] ?? 'see the output "WC3 Build"'
            // The game can't run here by itself: the setup (macOS), or the word on it (Linux).
            const setup = /can't run by itself/.test(first) && process.platform !== 'win32'
            window.showErrorMessage(`WC3: Test Map: ${first}`, ...(setup ? ['Set Up…'] : []), 'Open wc3.bni').then(go => {
                if (go === 'Set Up…') commands.executeCommand('wc3.testMapSetup', Uri.file(manifest))
                else if (go) window.showTextDocument(Uri.file(manifest))
            })
        }
        return {code: result.code, output: result.out + result.err}
    }

    const lenses = new EventEmitter()
    context.subscriptions.push(
        output,
        lenses,
        commands.registerCommand('wc3.testMap', arg => test(arg).catch(e => {
            window.showErrorMessage(`WC3: ${e.message}`)
        })),
        // Test Map's setup: Porting Kit's on macOS, the word on Linux.
        commands.registerCommand('wc3.testMapSetup', async arg => {
            const manifest = await pickManifest(arg, 'Set up Test Map for which project?')
            if (!manifest) return window.showWarningMessage('WC3: no wc3.bni in the workspace or above the active file.')
            return setup.open(manifest)
        }),
        // A menu passes the file and what it was run from (the editor
        // group's ids); a lens, the file and the build's name.
        commands.registerCommand('wc3.build', (arg, name) => run(arg, typeof name === 'string' ? name : undefined).catch(e => {
            window.showErrorMessage(`WC3: ${e.message}`)
        })),
        workspace.onDidChangeTextDocument(e => { if (path.basename(e.document.uri.fsPath) === 'wc3.bni') lenses.fire() }),
        languages.registerCodeLensProvider({language: 'bni', pattern: '**/wc3.bni'}, {
            onDidChangeCodeLenses: lenses.event,
            provideCodeLenses(doc) {
                return manifestBuilds(doc.getText()).map(b => new CodeLens(new Range(b.line, 0, b.line, 0), {
                    title: b.name ? `$(play) Build ${b.name}` : '$(play) Build (default)',
                    command: 'wc3.build',
                    arguments: [doc.uri, b.name],
                }))
            },
        }),
    )
}

module.exports = {
    /** @param {import('vscode').ExtensionContext} context */
    activate(context) {
        diagnostics = languages.createDiagnosticCollection('wc3')
        context.subscriptions.push(diagnostics, tokensChanged)
        // Before the language server: it doesn't need it.
        registerMcp(context)

        const command = serverBinaryPath(context)
        if (!command) return
        startServer(command, context)
        registerQueries(context)
        registerFixes(context)
        registerManifestPaths(context)
        registerMaps(context)
        registerTextures(context)
        registerObjects(context)
        registerSlk(context)
        const triggers = registerTrigger(context)
        registerPieces(context)
        registerTriggerExtract(context)
        registerTriggerJass(context)
        registerBuild(context, command)
        registerTxt2Preview(context)
        const worldEditors = registerWorldEditor(context, command, pickManifest, triggers)

        sendRoots()
        context.subscriptions.push(
            workspace.onDidChangeWorkspaceFolders(sendRoots),
            workspace.onDidOpenTextDocument(onOpen),
            workspace.onDidCloseTextDocument(onClose),
            workspace.onDidChangeTextDocument(onChange),
        )
        for (const doc of workspace.textDocuments) onOpen(doc)
        // What the tests in the editor (`e2e/`) reach.
        return {test: {triggerEditors, worldEditors, PIECE_SCHEME, OBJECTS_SCHEME}}
    },

    deactivate() {
        // Closing stdin is the server's signal to exit.
        if (server) {
            const proc = server
            server = null
            proc.stdin.end()
        }
    },
}
