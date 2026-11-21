'use strict'

/**
 * WC3 extension entry point: a thin shell around `wc3-server` (see
 * `../server`). All language logic is in Rust.
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
 * with `blp-rs` (`server/src/texture.rs`), and the page draws its mips.
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
 * In `wc3.bni`, the paths of `game=` and `sources=` complete from the disk
 * and every folder along them is a link that reveals that folder (in the
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
} = require('vscode')
const {encode, decode, FrameReader, QUERY, FS_OP, FS_FLAG, OBJECTS_OP} = require('./protocol.js')

/** The scheme of files inside maps. */
const MAP_SCHEME = 'wc3map'

/** Archives a path can lead into: maps, and the game's `.mpq`s (the
 * server's `map::ARCHIVES`). */
const MAP_NAME = /\.(w3[xm]|mpq)$/i

/** The scheme of object data shown as text. */
const OBJECTS_SCHEME = 'wc3obj'

/** The languages of object-data files (`package.json`), one per type. As
 * `wc3obj:` text they're BNI; a file opened as is is binary, not text. */
const OBJECT_LANGUAGES = new Set(['w3a', 'w3b', 'w3d', 'w3h', 'w3q', 'w3t', 'w3u'])

/** Language ids the server analyzes (its `Language::from_id`). */
const LANGUAGES = new Set(['jass', 'vjass', 'zinc', 'lua', 'angelscript', 'bni', 'slk', 'wts', ...OBJECT_LANGUAGES])

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
 * Resolves the `wc3-server` binary to spawn.
 *
 * Prefers the packaged, cross-compiled binary under `./bin/` (produced by
 * `server/build.sh` / `server/build-mac.sh`: `wc3-server-macos` /
 * `wc3-server-linux` / `wc3-server-windows.exe`). It lives inside the
 * extension folder itself, not `../server/bin`, because `vsce` only
 * packages files under the folder that holds `package.json`. Falls back to
 * a local dev build (`../server/target/{release,debug}/wc3-server`) so
 * day-to-day development doesn't require the full cross-compile.
 *
 * @param {import('vscode').ExtensionContext} context
 * @returns {string | null} null if the platform isn't supported.
 */
function serverBinaryPath(context) {
    const serverDir = path.join(context.extensionPath, '..', 'server')

    let platformSuffix
    switch (process.platform) {
        case 'win32':
            platformSuffix = 'windows.exe'
            break
        case 'darwin':
            platformSuffix = 'macos'
            break
        case 'linux':
            platformSuffix = 'linux'
            break
        default:
            window.showErrorMessage(`WC3: unsupported platform: ${process.platform}`)
            return null
    }

    const packaged = path.join(context.extensionPath, 'bin', `wc3-server-${platformSuffix}`)
    if (fs.existsSync(packaged)) return packaged

    const devName = process.platform === 'win32' ? 'wc3-server.exe' : 'wc3-server'
    const release = path.join(serverDir, 'target', 'release', devName)
    if (fs.existsSync(release)) return release

    return path.join(serverDir, 'target', 'debug', devName)
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
    return hasPath(doc.uri) || doc.uri.scheme === 'untitled'
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
    const answer = await query(kind, idOf(doc), pos, newName)
    if (!answer) throw new Error('WC3: the language server did not answer.')
    if (answer.text) throw new Error(answer.text)
    return answer.locations
}

/**
 * Go to definition, find references, hover, outline, workspace symbol
 * search and rename, all answered by the server's symbol index.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerQueries(context) {
    const selector = [...LANGUAGES].map(language => ({language}))
    const ask = async (kind, doc, pos) => (await query(kind, idOf(doc), pos))?.locations ?? []
    context.subscriptions.push(
        languages.registerDefinitionProvider(selector, {
            provideDefinition: async (doc, pos) => (await ask(QUERY.DEFINITION, doc, pos)).map(toLocation),
        }),
        languages.registerReferenceProvider(selector, {
            provideReferences: async (doc, pos, ctx) => {
                const found = (await ask(QUERY.REFERENCES, doc, pos)).map(toLocation)
                if (ctx.includeDeclaration) return found
                const decls = (await ask(QUERY.DEFINITION, doc, pos)).map(toLocation)
                return found.filter(r => !decls.some(d => d.uri.toString() === r.uri.toString() && d.range.isEqual(r.range)))
            },
        }),
        languages.registerHoverProvider(selector, {
            provideHover: async (doc, pos) => {
                const answer = await query(QUERY.HOVER, idOf(doc), pos)
                return answer && answer.text ? new Hover(new MarkdownString(answer.text)) : null
            },
        }),
        languages.registerDocumentSymbolProvider(selector, {
            provideDocumentSymbols: async doc => (await ask(QUERY.DOCUMENT_SYMBOLS, doc, {line: 0, character: 0})).map(toSymbol),
        }),
        languages.registerRenameProvider(selector, {
            prepareRename: async (doc, pos) => {
                const [here] = await askRename(QUERY.PREPARE_RENAME, doc, pos)
                if (!here) throw new Error('There\'s no symbol here to rename.')
                return {range: toLocation(here).range, placeholder: here.name}
            },
            provideRenameEdits: async (doc, pos, newName) => {
                let locations = (await askRename(QUERY.RENAME, doc, pos, newName)).map(toLocation)
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
 * @param {Uint8Array} data
 * @returns {Promise<import('./protocol.js').Texture>}
 */
function textureRequest(data) {
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
        send(encode.texture(request, data))
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
    <div class="group" id="channels"><span class="label">Channels</span></div>
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
            panel.iconPath = Uri.joinPath(context.extensionUri, 'icons', 'blp.svg')
            webview.html = html(webview)

            const load = async () => {
                try {
                    const data = await workspace.fs.readFile(uri)
                    webview.postMessage({type: 'texture', size: data.length, texture: await textureRequest(data)})
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
                await window.showTextDocument(doc, {viewColumn: panel.viewColumn, preview: false})
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

    context.subscriptions.push(
        changed,
        languages.registerInlayHintsProvider(selector, hints),
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
function registerTokens(msg, context) {
    const legend = new SemanticTokensLegend(msg.types, msg.modifiers)
    const provider = {
        onDidChangeSemanticTokens: tokensChanged.event,

        /** @param {import('vscode').TextDocument} doc */
        provideDocumentSemanticTokens(doc) {
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
    server = spawn(command, [], {stdio: ['pipe', 'pipe', 'pipe']})
    server.on('error', e => {
        window.showErrorMessage(
            `WC3: failed to start the language server (${e.message}). ` +
            'Run "cargo build" (or server/build.sh for a packaged binary) in the server/ crate first.',
        )
        server = null
    })
    server.on('exit', code => {
        if (server) console.error(`[wc3] server exited with code ${code}`)
        server = null
    })
    server.stderr.on('data', chunk => console.error(`[wc3-server] ${chunk.toString().trimEnd()}`))

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

module.exports = {
    /** @param {import('vscode').ExtensionContext} context */
    activate(context) {
        diagnostics = languages.createDiagnosticCollection('wc3')
        context.subscriptions.push(diagnostics, tokensChanged)

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

        sendRoots()
        context.subscriptions.push(
            workspace.onDidChangeWorkspaceFolders(sendRoots),
            workspace.onDidOpenTextDocument(onOpen),
            workspace.onDidCloseTextDocument(onClose),
            workspace.onDidChangeTextDocument(onChange),
        )
        for (const doc of workspace.textDocuments) onOpen(doc)
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
