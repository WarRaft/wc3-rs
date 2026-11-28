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
const CURRENT = ['format']

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
        {name: 'format', run: async () => {
            log('Format Document on JASS, vJASS (with a zinc block), Zinc and AngelScript: lines only moved left or right, none added or removed; Format Selection moves only the lines selected')
            const cases = [
                ['Fmt.j', 'function F takes nothing returns nothing\nlocal integer i = 0\nloop\nexitwhen i > 1\n  if i == 0 then\ncall BJDebugMsg("a\n  b")\n        else\nset i = i + 1\nendif\nendloop\n   endfunction\n',
                    'function F takes nothing returns nothing\n    local integer i = 0\n    loop\n        exitwhen i > 1\n        if i == 0 then\n            call BJDebugMsg("a\n  b")\n        else\n            set i = i + 1\n        endif\n    endloop\nendfunction\n'],
                ['Fmt.vj', 'library L\nstruct S\nmethod m takes nothing returns nothing\nendmethod\nendstruct\nendlibrary\n  //! zinc\nlibrary Z {\nfunction f() {\nx = 1;\n}\n}\n//! endzinc\n',
                    'library L\n    struct S\n        method m takes nothing returns nothing\n        endmethod\n    endstruct\nendlibrary\n//! zinc\nlibrary Z {\n    function f() {\n        x = 1;\n    }\n}\n//! endzinc\n'],
                ['Fmt.zn', 'library A {\nfunction f() {\nif (x)\ny();\n}\n}\n', 'library A {\n    function f() {\n        if (x)\n            y();\n    }\n}\n'],
                ['Fmt.as', 'void f(int k) {\nswitch (k) {\n  case 1:\nx();\nbreak;\n}\n}\n', 'void f(int k) {\n    switch (k) {\n    case 1:\n        x();\n        break;\n    }\n}\n'],
            ]
            for (const [name, text, want] of cases) {
                const file = path.join(root, name)
                fs.writeFileSync(file, text)
                const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(file))
                const editor = await vscode.window.showTextDocument(doc)
                editor.options = {insertSpaces: true, tabSize: 4}
                const lines = doc.lineCount
                // The server has it once it answers with its highlighting.
                await until(async () => (await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', doc.uri))?.data?.length, `${name} analyzed`, 30000)
                await until(async () => {
                    await vscode.commands.executeCommand('editor.action.formatDocument')
                    return doc.getText() !== text
                }, `${name} formatted`)
                check(doc.getText() === want, `${name} formatted:\n${doc.getText()}`)
                check(doc.lineCount === lines, `${name}: still ${lines} lines`)
                await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
            }
            // Format Selection: lines 3 and 4 of the Zinc file alone.
            const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(root, 'Fmt.zn')))
            const editor = await vscode.window.showTextDocument(doc)
            editor.options = {insertSpaces: false, tabSize: 4}
            editor.selection = new vscode.Selection(2, 0, 4, 0)
            await until(async () => {
                await vscode.commands.executeCommand('editor.action.formatSelection')
                return doc.isDirty
            }, 'the selection formatted')
            check(doc.getText() === 'library A {\nfunction f() {\n\t\tif (x)\n\t\t\ty();\n}\n}\n', `the selection alone, with tabs:\n${doc.getText()}`)
            await shot('format')
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
            await vscode.commands.executeCommand('workbench.action.closeAllEditors')
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
