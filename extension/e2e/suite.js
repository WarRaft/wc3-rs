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
const CURRENT = ['script-code']

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
        {name: 'script-code', run: async () => {
            log("Custom script in the trigger editor read as the trigger's code: an if in one action, its endif in another, an action between; tokens of the phrases' values and of code colored one way, the theme's colors")
            const file = path.join(root, 'triggers', 'Script.trg')
            fs.writeFileSync(file, '[variable]\nname="hero"\ntype="unit"\nsize=1\n\n[trigger]\nname="Script"\naction=CustomScriptCode("local integer i = 1")\naction=CustomScriptCode("if i > 0 then")\naction=DoNothing()\naction=CustomScriptCode("    call KillUnit(udg_hero)")\naction=CustomScriptCode("endif")\n')
            await vscode.commands.executeCommand('vscode.openWith', vscode.Uri.file(file), 'wc3.trigger')
            const editor = await until(() => api.triggerEditors.get(vscode.Uri.file(file).toString()), 'the trigger editor')
            // (The server reads the project's game files first.)
            // (A file just made: the editor asks till the server has it.)
            const view = await until(() => editor.state().view, 'the view', 60000)
            const lines = new Map()
            const walk = nodes => nodes.forEach(n => {
                lines.set(n.line, n)
                n.blocks.forEach(b => walk(b.nodes))
            })
            walk(view.actions)
            const code = line => lines.get(line)?.code
            check([8, 9, 11, 12].every(l => code(l)), 'each custom script action has its code read')
            check([8, 9, 11, 12].every(l => !code(l).errors.length), 'no errors: the if and its endif are one construct')
            check(code(12).tokens.some(t => t[3] === 'keyword' && t[1] === 0 && t[2] === 5), 'endif a keyword')
            check(code(9).tokens.some(t => t[3] === 'variable' && t[1] === 3), "if's i the local")
            check(code(11).tokens.some(t => t[3] === 'variable' && t[1] === 18), 'udg_hero a variable, as a variable of the phrases')
            const css = api.themeTokenCss()
            check(/\.tok\.tok-keyword \{ color: #/.test(css) && /\.tok\.tok-variable \{/.test(css), `the theme's colors of tokens:\n${css.split('\n').slice(0, 6).join('\n')}`)
            await shot('script-code')
            fs.writeFileSync(file, fs.readFileSync(file, 'utf8').replace('action=CustomScriptCode("endif")\n', ''))
            await until(() => editor.state().view?.actions?.some(n => n.code?.errors?.length), 'without its endif: an error', 60000)
            await vscode.commands.executeCommand('workbench.action.closeAllEditors')
            fs.rmSync(file)
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
