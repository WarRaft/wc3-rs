'use strict'

/**
 * WC3 extension entry point.
 *
 * Spawns the `wc3-server` Rust binary (see `../server`, which wraps
 * `jass-rs`, `zinc-rs`, `lua-rs` and `as-rs`) as a subprocess and talks to it over the same transport as
 * the older JASS-Tree-sitter-Rust extension: a local HTTP server on a
 * random port, handshake read from the process's first stdout line,
 * `?token=` auth, shutdown on stdin close. `serverClient.js` and
 * `httpTransport.js` implement that transport and are unchanged copies —
 * none of it is JASS-specific.
 *
 * Feature-wise this wires up, for JASS, Zinc, Lua, AngelScript and BNI documents (the document's
 * `languageId` picks the parser):
 * - diagnostics (parse errors + lint warnings) via `/document/update`;
 * - semantic highlighting via `/semantic/legend` + `/document/semanticTokens`;
 * - projects: each request carries the document's path, and the server reads
 *   the nearest `wc3.bni` above it (see the README, "Projects"). When a
 *   `wc3.bni` is saved, created or deleted, every open document is
 *   re-analyzed and re-highlighted. Everything else the old
 * extension's protocol supports (hover, completion, ...) has no
 * counterpart here yet — `wc3-server` doesn't implement those routes.
 */

const {
    EventEmitter,
    workspace,
    languages,
    window,
    Diagnostic,
    DiagnosticSeverity,
    Range,
    Position,
    SemanticTokens,
    SemanticTokensLegend,
} = require('vscode')

const path = require('path')
const fs = require('fs')
const {ServerClient} = require('./serverClient.js')

const DEBOUNCE_MS = 200

/** Language ids `wc3-server` can analyze (its `analyze` dispatch). */
const LANGUAGES = new Set(['jass', 'zinc', 'lua', 'angelscript', 'bni'])

/** The project manifest's file name (the server's `project::MANIFEST`). */
const MANIFEST = 'wc3.bni'

/** Fired when project settings change, so highlighting is requested again. */
const semanticTokensChanged = new EventEmitter()

/** @type {ServerClient | null} */
let client = null

/** @type {import('vscode').DiagnosticCollection | null} */
let diagnosticCollection = null

/** @type {Map<string, NodeJS.Timeout>} */
const updateTimers = new Map()

/**
 * Resolves the `wc3-server` binary to spawn.
 *
 * Prefers the packaged, cross-compiled binary under `./bin/` (produced by
 * `server/build.sh` / `server/build-mac.sh`, named like the older
 * extension's own `bin/` binaries: `wc3-server-macos` / `wc3-server-linux` /
 * `wc3-server-windows.exe`). It lives inside the extension folder itself —
 * not `../server/bin` — because `vsce` only packages files under the
 * folder that holds `package.json`. Falls back to a local dev build
 * (`../server/target/{release,debug}/wc3-server`) so day-to-day
 * development doesn't require running the full cross-compile every time.
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

/** @param {import('vscode').TextDocument} document */
function scheduleUpdate(document) {
    if (!LANGUAGES.has(document.languageId)) return
    const key = document.uri.toString()
    clearTimeout(updateTimers.get(key))
    updateTimers.set(key, setTimeout(() => {
        updateTimers.delete(key)
        updateDiagnostics(document).catch(e => console.error('[wc3] update failed:', e))
    }, DEBOUNCE_MS))
}

/**
 * The request body describing a document. `path` lets the server find the
 * document's project; unsaved and non-file documents have none.
 *
 * @param {import('vscode').TextDocument} document
 */
function documentParams(document) {
    return {
        text: document.getText(),
        languageId: document.languageId,
        path: document.uri.scheme === 'file' ? document.uri.fsPath : null,
    }
}

/** @param {import('vscode').TextDocument} document */
async function updateDiagnostics(document) {
    if (!client) return

    const result = await client.sendRequest('document/update', documentParams(document))
    if (!result || !Array.isArray(result.diagnostics)) return

    const diagnostics = result.diagnostics.map(d => {
        const line = Math.min(d.line0, Math.max(document.lineCount - 1, 0))
        const lineLength = document.lineAt(line).text.length
        const startChar = d.char0 ?? 0
        const endChar = d.endChar0 ?? lineLength
        const range = new Range(new Position(line, startChar), new Position(line, endChar))
        const severity = d.severity === 'error' ? DiagnosticSeverity.Error : DiagnosticSeverity.Warning
        return new Diagnostic(range, d.message, severity)
    })

    diagnosticCollection.set(document.uri, diagnostics)
}

/**
 * Registers semantic highlighting for every language in {@link LANGUAGES}.
 * The legend (VS Code's standard token types and modifiers) comes from the
 * server, so the Rust side is the single source of truth for the indices
 * its encoded token data uses.
 *
 * @param {import('vscode').ExtensionContext} context
 */
async function registerSemanticTokens(context) {
    const legendInfo = await client.sendRequest('semantic/legend', {})
    if (!legendInfo) return
    const legend = new SemanticTokensLegend(legendInfo.tokenTypes, legendInfo.tokenModifiers)

    const provider = {
        onDidChangeSemanticTokens: semanticTokensChanged.event,

        /** @param {import('vscode').TextDocument} document */
        async provideDocumentSemanticTokens(document) {
            if (!client) return null
            const result = await client.sendRequest('document/semanticTokens', documentParams(document))
            if (!result || !Array.isArray(result.data)) return null
            return new SemanticTokens(Uint32Array.from(result.data))
        },
    }

    const selector = [...LANGUAGES].map(language => ({language}))
    context.subscriptions.push(
        languages.registerDocumentSemanticTokensProvider(selector, provider, legend),
    )
}

/**
 * Re-analyzes and re-highlights every open document whenever a `wc3.bni`
 * changes on disk: its settings (such as the default JASS dialect) apply to
 * every file below it. The server re-reads a manifest by itself once its
 * modification time changes, so the extension only has to ask again.
 *
 * @param {import('vscode').ExtensionContext} context
 */
function watchManifests(context) {
    const watcher = workspace.createFileSystemWatcher(`**/${MANIFEST}`)
    const refresh = () => {
        for (const doc of workspace.textDocuments) scheduleUpdate(doc)
        semanticTokensChanged.fire()
    }
    watcher.onDidCreate(refresh)
    watcher.onDidChange(refresh)
    watcher.onDidDelete(refresh)
    context.subscriptions.push(watcher, semanticTokensChanged)
}

module.exports = {
    /** @param {import('vscode').ExtensionContext} context */
    async activate(context) {
        diagnosticCollection = languages.createDiagnosticCollection('wc3')
        context.subscriptions.push(diagnosticCollection)

        const binPath = serverBinaryPath(context)
        if (!binPath) return

        client = new ServerClient(binPath)
        try {
            await client.start()
        } catch (e) {
            window.showErrorMessage(
                `WC3: failed to start the language server (${e.message}). ` +
                'Run "cargo build" (or server/build.sh for a packaged binary) in the server/ crate first.',
            )
            client = null
            return
        }

        try {
            await registerSemanticTokens(context)
        } catch (e) {
            console.error('[wc3] semantic tokens unavailable:', e)
        }

        watchManifests(context)

        context.subscriptions.push(workspace.onDidOpenTextDocument(scheduleUpdate))
        context.subscriptions.push(workspace.onDidChangeTextDocument(e => scheduleUpdate(e.document)))
        context.subscriptions.push(workspace.onDidCloseTextDocument(doc => {
            const key = doc.uri.toString()
            clearTimeout(updateTimers.get(key))
            updateTimers.delete(key)
            diagnosticCollection.delete(doc.uri)
        }))

        for (const doc of workspace.textDocuments) scheduleUpdate(doc)
    },

    async deactivate() {
        for (const timer of updateTimers.values()) clearTimeout(timer)
        updateTimers.clear()
        if (client) {
            await client.stop()
            client = null
        }
    },
}
