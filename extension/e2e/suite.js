/**
 * The tests in the editor, run by `run.js` in a VS Code of its own: a
 * map's triggers and custom script, driven as a user would (the page's
 * messages through the extension's `test` export), each step checked on
 * the documents and written to `log.txt`.
 */
const vscode = require('vscode')
const {execFileSync} = require('child_process')
const fs = require('fs')
const path = require('path')

const out = process.env.E2E_OUT
const map = process.env.E2E_MAP
const log = line => fs.appendFileSync(path.join(out, 'log.txt'), line + '\n')
const sleep = ms => new Promise(r => setTimeout(r, ms))

/** Waits for `what` to be truthy (and gives it back), or fails saying `why`. */
async function until(what, why, ms = 20000) {
    const end = Date.now() + ms
    for (;;) {
        const v = await what()
        if (v) return v
        if (Date.now() > end) throw new Error(`timed out: ${why}`)
        await sleep(100)
    }
}

let shots = 0
/**
 * A screenshot of the screen, named: asked of `run.js` (a file it
 * watches), so the terminal's Screen Recording is the one it needs.
 */
async function shot(name) {
    await sleep(800)
    const file = path.join(out, `${String(++shots).padStart(2, '0')}-${name}.png`)
    const done = path.join(out, 'shot.done')
    fs.rmSync(done, {force: true})
    fs.writeFileSync(path.join(out, 'shot.request'), file)
    const end = Date.now() + 10000
    while (!fs.existsSync(done) && Date.now() < end) await sleep(100)
    const said = fs.existsSync(done) ? fs.readFileSync(done, 'utf8') : 'run.js did not answer'
    log(said ? `  no screenshot: ${said.trim()}` : `  screenshot ${path.basename(file)}`)
}

function check(ok, what) {
    if (!ok) throw new Error(`failed: ${what}`)
    log(`  ok: ${what}`)
}

/** The code lines of each trigger's piece of a custom script's text, by its markers. */
function piecesOf(text) {
    const lines = text.split('\n')
    const markers = []
    lines.forEach((l, i) => {
        const m = /^\s*\/\/\s*@wct\s+(\S+)/.exec(l)
        if (m) markers.push({line: i, word: m[1]})
    })
    return markers.map((m, k) => ({word: m.word, code: lines.slice(m.line + 1, markers[k + 1] ? markers[k + 1].line : lines.length).join('\n')}))
        .filter(m => m.word === 'trigger').map(m => m.code)
}

async function steps() {
    const ext = vscode.extensions.all.find(e => e.packageJSON.name === 'wc3' && e.packageJSON.publisher === 'WarRaft')
    const api = (await ext.activate()).test
    await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    // "Extensions are disabled" (the others are, on purpose): not in the screenshots.
    await vscode.commands.executeCommand('notifications.clearAll')

    log('the map built from the button over wc3.bni (the file and the editor group, as a menu passes them)')
    const manifest = vscode.Uri.file(path.join(path.dirname(map), 'wc3.bni'))
    await vscode.window.showTextDocument(manifest)
    const built = path.join(path.dirname(map), 'build', 'debug.w3x')
    fs.rmSync(built, {force: true})
    const result = await vscode.commands.executeCommand('wc3.build', manifest, {groupId: 1})
    if (!result || result.code !== 0) log(`  the build said:\n${result?.output ?? '(nothing)'}`)
    check(result && result.code === 0, 'the build succeeded')
    check(fs.existsSync(built), 'build/debug.w3x written')
    check(fs.statSync(built).size > 1000000, `build/debug.w3x (${Math.round(fs.statSync(built).size / 1024)} KB)`)
    await vscode.commands.executeCommand('workbench.action.closeAllEditors')

    log('the custom script opened as the Explorer opens it: the triggers come up')
    await vscode.commands.executeCommand('vscode.open', vscode.Uri.file(path.join(map, 'war3map.wct')).with({scheme: 'wc3map'}))
    const editor = await until(() => [...api.triggerEditors.values()][0], 'a trigger editor')
    const tree = await until(() => editor.state().tree, 'the tree')
    check(tree.triggers.length > 0, `${tree.triggers.length} triggers in the tree`)
    check(tree.wct && tree.wct.problem === null, 'the custom script goes with the triggers')
    const wct = editor.wct()
    check(!!wct, 'war3map.wct open beside, as a document')
    await shot('triggers')

    log("the custom script's problems: none of names World Editor's script declares")
    const wctDiags = await until(async () => {
        await sleep(1500)
        const d = vscode.languages.getDiagnostics(wct.uri)
        return d.length || (await sleep(3000), vscode.languages.getDiagnostics(wct.uri)), vscode.languages.getDiagnostics(wct.uri)
    }, 'the diagnostics of war3map.wct')
    const errors = wctDiags.filter(d => d.severity === vscode.DiagnosticSeverity.Error)
    const kinds = {}
    for (const d of wctDiags) kinds[d.code ?? d.message.replace(/'[^']*'/g, "'…'")] = (kinds[d.code ?? d.message.replace(/'[^']*'/g, "'…'")] ?? 0) + 1
    log(`  ${wctDiags.length} diagnostics, ${errors.length} errors; by kind: ${JSON.stringify(kinds)}`)
    for (const d of errors.slice(0, 8)) log(`    error ${d.range.start.line + 1}: ${d.message}`)
    check(!wctDiags.some(d => /'(gg_trg_|udg_)/.test(d.message)), "no trigger's or variable's name is unknown")

    const nth = tree.triggers.findIndex((t, k) => t.custom && !tree.wct.code[k][2]) + 1
    const name = tree.triggers[nth - 1].name
    log(`a trigger of custom text picked: ${nth} ${name}`)
    await editor.receive({type: 'pick', nth})
    const piece = await until(() => vscode.window.visibleTextEditors.find(e => e.document.uri.scheme === api.PIECE_SCHEME)?.document, 'its code opened')
    check(path.basename(piece.uri.fsPath) === `${name.replace(/[\\/:*?"<>|]/g, '_')}.j`, `opened as ${path.basename(piece.uri.fsPath)}`)
    const before = piecesOf(wct.getText())[nth - 1]
    check(piece.getText() === before.replace(/\n$/, ''), 'its text is its code, and only that')
    await shot('code')

    log('typed in the piece: in the custom script at once')
    let edit = new vscode.WorkspaceEdit()
    edit.insert(piece.uri, new vscode.Position(0, 0), '// e2e\n')
    await vscode.workspace.applyEdit(edit)
    await until(() => piecesOf(wct.getText())[nth - 1].startsWith('// e2e\n'), 'the custom script has it')
    check(true, 'the line is in its piece of war3map.wct')
    const others = piecesOf(wct.getText())
    check(others.filter(c => c.includes('// e2e')).length === 1, 'and in no other piece')

    log('an unknown function in the piece: its diagnostic is shown there')
    edit = new vscode.WorkspaceEdit()
    edit.insert(piece.uri, new vscode.Position(piece.lineCount, 0), '\nfunction E2eProbe takes nothing returns nothing\n    call NoSuchE2eFunction()\nendfunction')
    await vscode.workspace.applyEdit(edit)
    const diags = await until(() => vscode.languages.getDiagnostics(piece.uri).filter(d => d.message.includes('NoSuchE2eFunction')).length && vscode.languages.getDiagnostics(piece.uri), 'a diagnostic in the piece', 30000)
    const d = diags.find(d => d.message.includes('NoSuchE2eFunction'))
    check(piece.lineAt(d.range.start.line).text.includes('NoSuchE2eFunction'), `on its line in the piece (${d.range.start.line + 1}): ${d.message}`)
    await shot('diagnostic')

    log('the trigger moved to the top of its category: its code goes with it')
    const t = tree.triggers[nth - 1]
    const first = tree.triggers.findIndex(x => x.category === t.category) + 1
    if (first !== nth) {
        await editor.receive({type: 'moveTrigger', moved: nth, category: t.category, before: first})
        await until(() => piecesOf(wct.getText())[first - 1].startsWith('// e2e\n'), 'the code in the trigger\'s new place')
        check(true, `its piece is the ${first}th now`)
        const now = await until(() => editor.state().tree.triggers[first - 1]?.name === name && editor.state().tree, 'the tree follows')
        check(now.triggers[first - 1].name === name, 'the tree has it there')
        await sleep(500)
        check(piece.getText().startsWith('// e2e\n') && !piece.isClosed, 'the piece still shows its code')
    } else log('  (first in its category already)')

    log('saved from the piece: the map with both')
    const stamp = fs.statSync(map).mtimeMs
    await piece.save()
    await until(() => !wct.isDirty, 'war3map.wct saved')
    const triggers = vscode.workspace.textDocuments.find(x => /war3map\.wtg$/i.test(x.uri.path) && x.uri.scheme === api.OBJECTS_SCHEME)
    await until(() => triggers && !triggers.isDirty, 'war3map.wtg saved with it')
    check(fs.statSync(map).mtimeMs > stamp, 'the map on disk was written')
    await shot('saved')
}

module.exports.run = async () => {
    log(`e2e ${new Date().toISOString()}`)
    try {
        await steps()
        log('all passed')
    } catch (e) {
        log(`FAILED: ${e.stack || e.message}`)
        await shot('failed')
        throw e
    }
}
