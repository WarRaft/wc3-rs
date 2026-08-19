'use strict'

/**
 * WC3 extension entry point.
 *
 * Spawns the `wc3-server` Rust binary (see `../server`, which wraps
 * `jass-rs`) as a subprocess and talks to it over the same transport as
 * the older JASS-Tree-sitter-Rust extension: a local HTTP server on a
 * random port, handshake read from the process's first stdout line,
 * `?token=` auth, shutdown on stdin close. `serverClient.js` and
 * `httpTransport.js` implement that transport and are unchanged copies —
 * none of it is JASS-specific.
 *
 * Feature-wise this only wires up diagnostics (parse errors + lint
 * warnings) for now, via `/document/update`. Everything else the old
 * extension's protocol supports (hover, completion, ...) has no
 * counterpart here yet — `wc3-server` doesn't implement those routes.
 */

const {
    workspace,
    languages,
    window,
    Diagnostic,
    DiagnosticSeverity,
    Range,
    Position,
} = require('vscode')

const path = require('path')
const {ServerClient} = require('./serverClient.js')

const DEBOUNCE_MS = 200

/** @type {ServerClient | null} */
let client = null

/** @type {import('vscode').DiagnosticCollection | null} */
let diagnosticCollection = null

/** @type {Map<string, NodeJS.Timeout>} */
const updateTimers = new Map()

/**
 * Path to the `wc3-server` binary. There's no packaged/prebuilt binary yet
 * (unlike the older extension, which ships one per platform under `bin/`)
 * — this assumes the dev layout where `extension/` and `server/` are
 * sibling directories in the same repo, and that `cargo build` has already
 * been run in `server/`.
 * @param {import('vscode').ExtensionContext} context
 */
function serverBinaryPath(context) {
    const name = process.platform === 'win32' ? 'wc3-server.exe' : 'wc3-server'
    return path.join(context.extensionPath, '..', 'server', 'target', 'debug', name)
}

/** @param {import('vscode').TextDocument} document */
function scheduleUpdate(document) {
    if (document.languageId !== 'jass') return
    const key = document.uri.toString()
    clearTimeout(updateTimers.get(key))
    updateTimers.set(key, setTimeout(() => {
        updateTimers.delete(key)
        updateDiagnostics(document).catch(e => console.error('[wc3] update failed:', e))
    }, DEBOUNCE_MS))
}

/** @param {import('vscode').TextDocument} document */
async function updateDiagnostics(document) {
    if (!client) return

    const result = await client.sendRequest('document/update', {text: document.getText()})
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

module.exports = {
    /** @param {import('vscode').ExtensionContext} context */
    async activate(context) {
        diagnosticCollection = languages.createDiagnosticCollection('jass')
        context.subscriptions.push(diagnosticCollection)

        client = new ServerClient(serverBinaryPath(context))
        try {
            await client.start()
        } catch (e) {
            window.showErrorMessage(
                `WC3: failed to start the language server (${e.message}). ` +
                'Did you run "cargo build" in the server/ crate?',
            )
            client = null
            return
        }

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
