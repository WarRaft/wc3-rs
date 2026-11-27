/**
 * The tests in the editor, run by `run.js` in a VS Code of its own: the
 * World Editor driven as a user would (the page's messages through the
 * extension's `test` export), each step checked and written to `log.txt`.
 * Only the work in hand has steps: a step goes when its work is done.
 */
const vscode = require('vscode')
const fs = require('fs')
const path = require('path')

/**
 * The steps of the work in hand (`node e2e/run.js current`); `all` runs
 * every step.
 */
const CURRENT = ['fdf']

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

/**
 * The steps, in the order they run, each its name (`run.js`'s list),
 * whether it works in the World Editor (opened before the first of
 * those, closed after the last: `open`, `close`) and the steps it needs
 * run before it.
 */
async function steps(only) {
    const ext = vscode.extensions.all.find(e => e.packageJSON.name === 'wc3' && e.packageJSON.publisher === 'WarRaft')
    const api = (await ext.activate()).test
    await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    // "Extensions are disabled" (the others are, on purpose): not in the screenshots.
    await vscode.commands.executeCommand('notifications.clearAll')
    const manifest = vscode.Uri.file(path.join(path.dirname(map), 'wc3.bni'))
    const root = path.dirname(manifest.fsPath)
    let we
    const open = async () => {
        log('the World Editor opened from the button over wc3.bni: the map shown, its terrain drawn')
        we = await vscode.commands.executeCommand('wc3.editor.open', manifest, {groupId: 1})
        check(!!we && api.worldEditors.get(root) === we, 'one editor, of the project')
        // (Reforged's storage, opened cold, takes a while: E2E_HD.)
        await until(() => we.map || we.error, 'the page shows the map', 180000)
        check(!!we.map, `the map shown${we.error ? `, not: ${we.error}` : ''}`)
        await until(() => we.drawn || we.error, 'the terrain drawn', 240000)
        check(!!we.drawn && !we.error, `the terrain drawn${we.error ? `, not: ${we.error}` : ''}`)
    }
    const close = async () => {
        await vscode.commands.executeCommand('wc3.editor.close', {root})
        check(!api.worldEditors.has(root), 'closed: no editor of the project')
        await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    }
    const STEPS = [
        {name: 'fdf', run: async () => {
            log('a frame definition file (.fdf): its language, its colors from the server by a statement\'s shape, a missing comma an error at once (unsaved), gone when it\'s typed')
            const file = path.join(root, 'UI', 'FrameDef', 'Test.fdf')
            fs.mkdirSync(path.dirname(file), {recursive: true})
            fs.writeFileSync(file, [
                '// A test frame',
                'IncludeFile "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",',
                'Frame "GLUETEXTBUTTON" "TestButton" INHERITS WITHCHILDREN "EscMenuButtonTemplate" {',
                '    Width 0.228,',
                '    ButtonPushedTextOffset 0.002f -0.002f,',
                '    SetPoint TOPLEFT, "TestParent", TOPLEFT, 0.0, -0.03,',
                '    Texture {',
                '        File "Test",',
                '    }',
                '}',
                '',
            ].join('\n'))
            const uri = vscode.Uri.file(file)
            const doc = await vscode.workspace.openTextDocument(uri)
            const editor = await vscode.window.showTextDocument(doc)
            check(doc.languageId === 'fdf', `the language: ${doc.languageId}`)
            const legend = await until(() => vscode.commands.executeCommand('vscode.provideDocumentSemanticTokensLegend', uri), 'the legend')
            const tokens = await until(async () => {
                const t = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri)
                return t && t.data.length ? t : null
            }, 'the colors')
            // Decoded: line (1-based), column (1-based), text, type.
            const got = []
            for (let i = 0, line = 0, col = 0; i < tokens.data.length; i += 5) {
                const [dl, dc, len, ty] = tokens.data.slice(i, i + 4)
                line += dl
                col = dl ? dc : col + dc
                got.push(`${line + 1}:${doc.lineAt(line).text.slice(col, col + len)}=${legend.tokenTypes[ty]}`)
            }
            const want = ['1:// A test frame=comment', '2:IncludeFile=property', '3:Frame=keyword', '3:"GLUETEXTBUTTON"=string', '3:INHERITS=enumMember', '3:{=operator',
                '5:0.002f=number', '5:-0.002f=number', '6:SetPoint=property', '6:TOPLEFT=enumMember', '6:-0.03=number', '7:Texture=keyword', '8:File=property', '10:}=operator']
            const missing = want.filter(w => !got.includes(w))
            check(missing.length === 0, `colored: ${want.length} checked${missing.length ? `, missing ${missing.join(' ')} of ${got.join(' ')}` : ''}`)
            await until(() => vscode.languages.getDiagnostics(uri).length === 0 || null, 'no problems', 5000).catch(() => {})
            check(vscode.languages.getDiagnostics(uri).length === 0, `no problems: ${vscode.languages.getDiagnostics(uri).map(d => d.message).join('; ')}`)
            // Width's comma out: an error on its line, unsaved.
            const comma = new vscode.Range(3, 15, 3, 16)
            check(doc.getText(comma) === ',', `the comma after Width: ${doc.getText(comma)}`)
            await editor.edit(e => e.delete(comma))
            const problems = await until(() => vscode.languages.getDiagnostics(uri).filter(d => d.severity === vscode.DiagnosticSeverity.Error).length ? vscode.languages.getDiagnostics(uri) : null, 'the missing comma reported')
            check(problems.length === 1 && problems[0].range.start.line === 3 && problems[0].message.includes("expected ','"), `one error, on Width's line: ${problems.map(d => `${d.range.start.line + 1}: ${d.message}`).join('; ')}`)
            await shot('fdf-missing-comma')
            await editor.edit(e => e.insert(new vscode.Position(3, 15), ','))
            await until(() => vscode.languages.getDiagnostics(uri).length === 0, 'the error gone')
            check(true, 'the comma typed back: no problems')
            await shot('fdf')
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
        }},
    ]
    // What's run: `only`'s names (all without it), with what they need.
    const names = new Set(only ?? STEPS.map(st => st.name))
    for (const n of names) if (!STEPS.some(st => st.name === n)) throw new Error(`no step ${n}: ${STEPS.map(st => st.name).join(', ')}`)
    for (const st of [...STEPS].reverse()) if (names.has(st.name)) for (const n of st.needs ?? []) names.add(n)
    const chosen = STEPS.filter(st => names.has(st.name))
    log(`steps: ${chosen.map(st => st.name).join(', ')}`)
    let opened = false
    for (const st of chosen) {
        if (st.editor && !opened) {
            await open()
            opened = true
        } else if (!st.editor && opened) {
            await close()
            opened = false
        }
        await st.run()
    }
    if (opened) await close()
}

module.exports.run = async () => {
    log(`e2e ${new Date().toISOString()}`)
    try {
        const which = process.env.E2E_STEPS || 'all'
        await steps(which === 'all' ? null : which === 'current' ? CURRENT : which.split(','))
        log('all passed')
    } catch (e) {
        log(`FAILED: ${e.stack || e.message}`)
        await shot('failed')
        throw e
    }
}
