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
const CURRENT = ['layout']

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
        {name: 'layout', editor: true, run: async () => {
            log('the page: the map\'s name a line of its own; under it the edits, the map\'s windows, the time, the layers, the controls and donate, icons; the view below; the ground under the mouse on the bottom line; a resize followed; the view kept per map between runs; closed and opened again at once')
            const layout = async ask => {
                we.layout = undefined
                await we.panel.webview.postMessage({type: 'layout', ...ask})
                return until(() => we.layout, 'the layout')
            }
            await we.panel.webview.postMessage({type: 'look', x: 96, y: 64, distance: 60, horizontal: 0, vertical: -1.2})
            let l = await layout({hover: [0, 0]})
            const [title, tools] = l.bars
            check(l.bars.length === 2 && title.className.includes('title-bar') && tools.className.includes('tool-bar'), `two bars: ${l.bars.map(b => b.className).join(', ')}`)
            const line = Math.max(l.name.bottom - l.name.top, l.meta.bottom - l.meta.top)
            check(line < 30, `the name and the map's size, tileset, author on one line: ${line} px high`)
            check(l.toggles.top >= l.name.bottom && l.view.top >= l.toggles.bottom, `the name above the buttons, the view below both (${l.name.bottom} ≤ ${l.toggles.top}, ${l.toggles.bottom} ≤ ${l.view.top})`)
            const order = l.bar.map(b => b.id || b.tip).join(' ')
            check(/^undo redo save reload test-map map-open triggers-open info-open constants-open skin-open imports-open objects-open Pathing .* Day cycle help-open donate$/.test(order), `the second line, in order: ${order}`)
            check(l.bar.every(b => b.icon && !b.text && b.tip), `its buttons icons, a tooltip each (${l.bar.filter(b => !b.icon || b.text || !b.tip).map(b => b.id || b.tip).join(', ') || 'all'})`)
            check(l.status.top >= l.view.bottom && l.status.bottom <= l.window.height, `the bottom line under the view (${l.view.bottom} ≤ ${l.status.top}, ${l.status.bottom} ≤ ${l.window.height})`)
            check(/^X -?[\d\u2009]+ {2}Y -?[\d\u2009]+ {2}Z -?[\d\u2009]+ {2}· {2}Cell \d+, \d+$/.test(l.cursor), `the ground under the mouse: ${l.cursor}`)
            check(l.hoursAfter, 'the time after the layers\' buttons')
            check(l.panels.every(b => b === 'border-box'), `the panels over the view inside it, as far from its bottom as from its top: ${l.panels.join(', ')}`)
            // The time dragged while the day goes round: from there, the slider not taken back.
            l = await layout({drag: 6})
            check(Math.abs(l.now - 6) < 0.1 && Math.abs(l.slider - 6) < 0.1, `the day going round from where the slider was dragged: ${l.now.toFixed(2)} hours, the slider at ${l.slider}`)
            await we.panel.webview.postMessage({type: 'show', show: {cycle: false}})
            await we.panel.webview.postMessage({type: 'hours', hours: 12})
            l = await layout({help: true})
            check(l.help, 'the controls\' window opened')
            await shot('world-editor-help')
            l = await layout({help: false})
            check(!l.help, 'the controls\' window closed')
            // The panel at the bottom opened, then closed: the view follows.
            const before = l.view
            await vscode.commands.executeCommand('workbench.action.togglePanel')
            await sleep(500)
            l = await layout({})
            check(l.view.bottom - l.view.top !== before.bottom - before.top && l.status.bottom <= l.window.height, `a resize followed: the view ${before.bottom - before.top} then ${l.view.bottom - l.view.top} high, the bottom line inside`)
            check(Math.abs(l.buffer.height - (l.view.bottom - l.view.top) * l.buffer.dpr) <= 2, `the view's buffer resized: ${l.buffer.height}`)
            await shot('world-editor-resized')
            await vscode.commands.executeCommand('workbench.action.togglePanel')
            // The layers and the time set, the editor closed and opened again: as they were.
            const fog = !l.bar.find(b => b.tip === 'Fog').on
            await we.panel.webview.postMessage({type: 'show', show: {fog}})
            await we.panel.webview.postMessage({type: 'hours', hours: 18.5})
            await sleep(300)
            // Opened again at once: it waits for the last process to let the project go.
            await close()
            await open()
            l = await layout({})
            check(l.bar.find(b => b.tip === 'Fog').on === fog && l.hours === 18.5, `the view kept for the map: fog ${fog ? 'on' : 'off'}, ${l.hours} hours`)
            await we.panel.webview.postMessage({type: 'show', show: {fog: !fog}})
            await we.panel.webview.postMessage({type: 'hours', hours: 12})
            await sleep(300)
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
