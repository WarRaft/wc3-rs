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
 * so they're shown under the `wc3map:` scheme with that path, read-only,
 * their text asked from the server.
 *
 * The Explorer has a tree of the workspace's maps: each opens like a
 * folder into the files inside it (the server lists them), and a text
 * file opens read-only like the scripts above.
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
    ThemeIcon,
    TreeItem,
    TreeItemCollapsibleState,
} = require('vscode')
const {encode, decode, FrameReader, QUERY} = require('./protocol.js')

/** The scheme of files inside maps. */
const MAP_SCHEME = 'wc3map'

/** Archives a path can lead into: maps, and the game's `.mpq`s (the
 * server's `map::ARCHIVES`). */
const MAP_NAME = /\.(w3[xm]|mpq)$/i

/** Language ids the server analyzes (its `Language::from_id`). */
const LANGUAGES = new Set(['jass', 'zinc', 'lua', 'angelscript', 'bni'])

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
    for (let dir = path.dirname(file); dir !== path.dirname(dir); dir = path.dirname(dir)) {
        if (!MAP_NAME.test(dir)) continue
        try {
            if (fs.statSync(dir).isFile()) return true
        } catch {
            // Not there: keep looking above.
        }
    }
    return false
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
    return LANGUAGES.has(doc.languageId) && (hasPath(doc.uri) || doc.uri.scheme === 'untitled')
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
    const file = hasPath(doc.uri) ? doc.uri.fsPath : ''
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
                const edit = new WorkspaceEdit()
                for (const l of await askRename(QUERY.RENAME, doc, pos, newName)) {
                    const loc = toLocation(l)
                    edit.replace(loc.uri, loc.range, newName)
                }
                return edit
            },
        }),
        workspace.registerTextDocumentContentProvider(MAP_SCHEME, {
            provideTextDocumentContent: async uri =>
                (await query(QUERY.CONTENT, uri.fsPath, {line: 0, character: 0}))?.text ?? '',
        }),
        languages.registerWorkspaceSymbolProvider({
            provideWorkspaceSymbols: async text =>
                ((await query(QUERY.WORKSPACE_SYMBOLS, '', {line: 0, character: 0}, text))?.locations ?? []).map(toSymbol),
        }),
    )
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

/** The id of the tree of maps (`package.json`). */
const MAPS_VIEW = 'wc3.maps'

/** Maps the tree lists. */
const MAP_GLOB = '**/*.{w3x,w3m,W3X,W3M}'

/**
 * The tree of maps in the Explorer (see the header). A node is
 * `{map}` (a map file), `{map, folder}` (a folder inside it, `\`-separated
 * as in the archive) or `{map, file}` (a file: its name in the archive, its
 * path, whether it's text, and whether it's a script the game doesn't
 * run).
 *
 * @param {import('vscode').ExtensionContext} context
 */
function registerMapTree(context) {
    const changed = new EventEmitter()
    /** map path → Promise of its files: [{name, id, text}]. */
    const listings = new Map()
    const filesOf = map => {
        if (!listings.has(map)) {
            listings.set(map, query(QUERY.ARCHIVE_FILES, map, {line: 0, character: 0}).then(answer =>
                (answer?.locations ?? []).map(l => ({name: l.name.replace(/\//g, '\\'), id: l.id, text: l.container !== 'binary', unused: l.container === 'unused'}))))
        }
        return listings.get(map)
    }
    const refresh = () => {
        listings.clear()
        changed.fire(undefined)
    }
    const byName = (a, b) => a.localeCompare(b, undefined, {sensitivity: 'base'})

    const provider = {
        onDidChangeTreeData: changed.event,

        async getChildren(node) {
            if (!node) {
                const found = await workspace.findFiles(MAP_GLOB)
                return found
                    .map(uri => uri.fsPath)
                    .sort((a, b) => byName(path.basename(a), path.basename(b)) || byName(a, b))
                    .map(map => ({map}))
            }
            if (node.file) return []
            // What's directly inside the map or the folder.
            const prefix = node.folder ? node.folder + '\\' : ''
            const folders = new Map()
            const files = []
            for (const file of await filesOf(node.map)) {
                if (!file.name.toLowerCase().startsWith(prefix.toLowerCase())) continue
                const rest = file.name.slice(prefix.length)
                const cut = rest.search(/[\\/]/)
                if (cut < 0) files.push({map: node.map, file})
                else {
                    const folder = prefix + rest.slice(0, cut)
                    const key = folder.toLowerCase()
                    if (!folders.has(key)) folders.set(key, {map: node.map, folder})
                }
            }
            const leaf = n => (n.folder ?? n.file.name).split(/[\\/]/).pop()
            return [
                ...[...folders.values()].sort((a, b) => byName(leaf(a), leaf(b))),
                ...files.sort((a, b) => byName(leaf(a), leaf(b))),
            ]
        },

        getTreeItem(node) {
            if (!node.folder && !node.file) {
                const item = new TreeItem(path.basename(node.map), TreeItemCollapsibleState.Collapsed)
                item.id = node.map
                item.iconPath = new ThemeIcon('package')
                item.description = workspace.asRelativePath(path.dirname(node.map))
                item.tooltip = node.map
                item.resourceUri = Uri.file(node.map)
                item.contextValue = 'wc3.map'
                return item
            }
            if (node.folder) {
                const item = new TreeItem(node.folder.split('\\').pop(), TreeItemCollapsibleState.Collapsed)
                item.id = `${node.map}|${node.folder.toLowerCase()}`
                item.iconPath = ThemeIcon.Folder
                item.resourceUri = Uri.file(path.join(node.map, ...node.folder.split('\\'))).with({scheme: MAP_SCHEME})
                return item
            }
            const {file} = node
            const uri = Uri.file(file.id).with({scheme: MAP_SCHEME})
            const item = new TreeItem(uri, TreeItemCollapsibleState.None)
            item.id = `${node.map}|${file.name.toLowerCase()}`
            item.iconPath = ThemeIcon.File
            item.tooltip = file.name
            if (file.text) item.command = {command: 'vscode.open', title: 'Open', arguments: [uri]}
            else item.description = 'binary'
            if (file.unused) {
                item.description = 'unused'
                item.tooltip = `${file.name}: not run, Scripts\\war3map.j is`
            }
            return item
        },
    }

    const watcher = workspace.createFileSystemWatcher(MAP_GLOB)
    context.subscriptions.push(
        changed,
        watcher,
        watcher.onDidCreate(refresh),
        watcher.onDidDelete(refresh),
        watcher.onDidChange(refresh),
        workspace.onDidChangeWorkspaceFolders(refresh),
        window.createTreeView(MAPS_VIEW, {treeDataProvider: provider, showCollapseAll: true}),
        commands.registerCommand('wc3.maps.refresh', refresh),
        commands.registerCommand('wc3.maps.reveal', node => node?.map && commands.executeCommand('revealInExplorer', Uri.file(node.map))),
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
            else if (msg.kind === 'answer') {
                const done = pendingQueries.get(msg.request)
                pendingQueries.delete(msg.request)
                if (done) done(msg)
            } else if (msg.kind === 'legend') registerTokens(msg, context)
        }
    })
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
        registerManifestPaths(context)
        registerMapTree(context)

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
