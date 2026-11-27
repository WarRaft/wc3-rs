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

/**
 * The steps of the work in hand (`node e2e/run.js current`): moved on as
 * the work does; `all` runs every step.
 */
const CURRENT = ['info']

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
    let we, drawn
    const open = async () => {
        log('the World Editor opened from the button over wc3.bni: the map, its name, its terrain drawn; opened again: the same one')
        we = await vscode.commands.executeCommand('wc3.editor.open', manifest, {groupId: 1})
        check(!!we && api.worldEditors.get(root) === we, 'one editor, of the project')
        const shown = await until(() => we.map || we.error, 'the page shows the map')
        check(!!we.map, `the map shown${we.error ? `, not: ${we.error}` : ''}`)
        check(shown.width > 1 && shown.height > 1, `its terrain: ${shown.width} × ${shown.height} points, "${shown.name}"`)
        check(we.panel.title === (shown.name || path.basename(root)), `the tab is the map's name: ${we.panel.title}`)
        drawn = await until(() => we.drawn || we.error, 'the terrain drawn', 60000)
        check(!!we.drawn && !we.error, `the terrain drawn${we.error ? `, not: ${we.error}` : ''}`)
        check(drawn.layers && drawn.water > 0 && drawn.error === 0, `with its ground textures and ${drawn.water} water frames, no GL error (${drawn.error})`)
        check(drawn.cliffs > 1000, `its cliffs and ramps: ${drawn.cliffs}`)
        check(drawn.objects > 10000, `what it places: ${drawn.objects} doodads, destructables, units and items`)
        check(drawn.animated > 10, `${drawn.animated} of their models move (their Stand), ${drawn.billboards} with billboards, ${drawn.geosetAnimated} with geosets shown, hidden or tinted as it goes`)
        check(drawn.emitters > 0 && drawn.error === 0, `${drawn.emitters} particle emitters (${drawn.tails} with tails, ${drawn.modelSpace} in the model's space), ${drawn.ribbons} ribbons, no GL error`)
        const lit = await until(() => we.light, 'the light')
        check(lit.day && lit.shadows > 0, `the day and night of its tileset, its shadows (${lit.shadows} wide)${lit.fog ? ', its fog' : ''}`)
        const again = await vscode.commands.executeCommand('wc3.editor.open', {root})
        check(again === we && api.worldEditors.size === 1, 'opened again from the WC3 Editors view: the same editor')
    }
    const close = async () => {
        await vscode.commands.executeCommand('wc3.editor.close', {root})
        check(!api.worldEditors.has(root), 'closed: no editor of the project')
        await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    }
    const STEPS = [
        {name: 'build', run: async () => {
            log('the map built from the button over wc3.bni (the file and the editor group, as a menu passes them): the default build')
            await vscode.window.showTextDocument(manifest)
            // Two [build]s, one the default (no name): no problem of them.
            await sleep(2000)
            const said = vscode.languages.getDiagnostics(manifest)
            check(!said.length, `wc3.bni has no problem${said.length ? `: ${said.map(d => d.message).join(' | ')}` : ''}`)
            const built = path.join(path.dirname(map), 'build', 'map.w3x')
            fs.rmSync(built, {force: true})
            const result = await vscode.commands.executeCommand('wc3.build', manifest, {groupId: 1})
            if (!result || result.code !== 0) log(`  the build said:\n${result?.output ?? '(nothing)'}`)
            check(result && result.code === 0, 'the build succeeded')
            check(fs.existsSync(built), 'the default build: build/map.w3x written')
            check(fs.statSync(built).size > 1000000, `build/map.w3x (${Math.round(fs.statSync(built).size / 1024)} KB)`)
            await vscode.commands.executeCommand('workbench.action.closeAllEditors')
        }},
        {name: 'look', editor: true, run: async () => {
            log('the view: the map, its cliffs, no model blinking, a start location, tails, midnight; a click picks a doodad')
            await shot('world-editor')
            await we.panel.webview.postMessage({type: 'look', x: 30, y: 100, distance: 32, horizontal: 0.4, vertical: -0.85})
            await shot('world-editor-cliffs')
            for (const animate of [false, true]) {
                we.flicker = undefined
                await we.panel.webview.postMessage({type: 'flicker', frames: 150, animate})
                const fl = await until(() => we.flicker, 'the frames read', 60000)
                log(`  animate ${animate}: ${fl.blinks} blinks in ${fl.frames} frames of ${fl.blocks} blocks ${JSON.stringify(fl.where)}`)
                if (animate) check(fl.blinks === 0, `no model blinks: ${fl.blinks} blinks in ${fl.frames} frames`)
            }
            we.focused = undefined
            check(drawn.startLocations > 0, `its start locations drawn: ${drawn.startLocations}`)
            we.focused = undefined
            await we.panel.webview.postMessage({type: 'focusEffect', path: 'StartLocation', distance: 5})
            await until(() => we.focused !== undefined, 'focused')
            await sleep(500)
            await shot('world-editor-start-location')
            await we.panel.webview.postMessage({type: 'unfocus'})
            we.focused = undefined
            await we.panel.webview.postMessage({type: 'focusEffect', tails: true})
            const focused = await until(() => we.focused !== undefined && we.focused, 'focused')
            check(!!focused, `the camera over a thing whose particles have tails: ${focused}`)
            await sleep(800)
            await shot('world-editor-tails')
            await we.panel.webview.postMessage({type: 'unfocus'})
            await we.panel.webview.postMessage({type: 'hours', hours: 0})
            await shot('world-editor-midnight')
            await we.panel.webview.postMessage({type: 'hours', hours: 12})
            // The map's first doodad, an arched door, from above: a click on it.
            await we.panel.webview.postMessage({type: 'look', world: true, x: -2708, y: 2670, z: 691, distance: 12, horizontal: 0, vertical: -1.5})
            await sleep(500)
            await we.panel.webview.postMessage({type: 'click', x: 0, y: 0})
            const picked = await until(() => we.picked !== undefined && we.picked, 'a click picks')
            check(/^arched door \(D011\) · doodad · .* war3map\.doo #0( · in .*)?$/.test(picked), `picked: ${picked}`)
            await shot('world-editor-picked')
        }},
        {name: 'overlays', editor: true, run: async () => {
            log('the overlays: pathing, regions, cameras, bounds, the minimap; a click in a region says it')
            const ov = await until(() => we.overlays, 'the overlays')
            check(ov.pathing > 0 && ov.regions > 0 && ov.cameras > 0 && ov.bounds > 0 && ov.minimap > 0, `pathing ${ov.pathing} wide, ${ov.regions} regions, ${ov.cameras} cameras, ${ov.bounds} points of bounds, a minimap ${ov.minimap} wide`)
            check(ov.destructables > 0, `the destructables' pathing: ${ov.destructables} cells, ${ov.decks} of bridges' decks`)
            await we.panel.webview.postMessage({type: 'show', show: {pathing: true, destructables: true, regions: true, cameras: true, bounds: true, minimap: true}})
            await we.panel.webview.postMessage({type: 'look', x: 96, y: 64, distance: 150, horizontal: 0, vertical: -1.1})
            await shot('world-editor-overlays')
            // A bridge over the falls: its deck walked on, its rails not.
            await we.panel.webview.postMessage({type: 'show', show: {regions: false}})
            await we.panel.webview.postMessage({type: 'look', x: 27, y: 78, distance: 18, horizontal: 0, vertical: -1.4})
            await sleep(300)
            await shot('world-editor-bridge')
            await we.panel.webview.postMessage({type: 'show', show: {regions: true}})
            we.picked = undefined
            await we.panel.webview.postMessage({type: 'look', world: true, x: ov.region.x, y: ov.region.y, distance: 6, horizontal: 0, vertical: -1.55})
            await sleep(500)
            await we.panel.webview.postMessage({type: 'click', x: 0, y: 0})
            const inRegion = await until(() => we.picked !== undefined && we.picked, 'a click in a region')
            check(inRegion.includes(`in ${ov.region.name}`), `picked in ${ov.region.name}: ${inRegion}`)
            await we.panel.webview.postMessage({type: 'show', show: {pathing: false, regions: false, cameras: false}})
        }},
        {name: 'terrain', editor: true, run: async () => {
            log('the terrain edited: a stroke raises it, undo and redo, a texture, saved into the map with the pathing made again')
            const stamp0 = fs.statSync(map).mtimeMs
            await we.panel.webview.postMessage({type: 'look', x: 60, y: 60, distance: 30, horizontal: 0, vertical: -1.2})
            await sleep(300)
            we.state = undefined
            await we.panel.webview.postMessage({type: 'brush', brush: {tool: 'raise', size: 8, strength: 1}, points: [[0, 0], [0.05, 0], [0.1, 0]]})
            await until(() => we.state?.undo, 'the stroke done')
            check(we.state.dirty && we.panel.title.startsWith('● '), `unsaved: the tab says so (${we.panel.title})`)
            await shot('world-editor-raised')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.state?.redo && !we.state.undo, 'undone')
            await we.panel.webview.postMessage({type: 'redo'})
            await until(() => we.state?.undo && !we.state.redo, 'redone')
            await we.panel.webview.postMessage({type: 'brush', brush: {tool: 'texture', size: 6}, points: [[-0.2, -0.2]]})
            await until(() => we.state && !we.state.redo && we.state.message === '' && we.state.undo, 'the texture laid')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const saved = await until(() => we.state?.message && we.state, 'saved')
            check(!saved.dirty && /^Saved .*map\.w3x.war3map\.w3e, .*map\.w3x.war3map\.wpm$/.test(saved.message), saved.message)
            check(fs.statSync(map).mtimeMs > stamp0 && !we.panel.title.startsWith('● '), 'the map on disk written, the tab clean')
        }},
        {name: 'phoenix', editor: true, run: async () => {
            log("a phoenix placed: its ribbons drawn; undone")
            await we.panel.webview.postMessage({type: 'look', x: 40, y: 40, distance: 12, horizontal: 0, vertical: -1.5})
            await sleep(300)
            we.state = undefined
            await we.panel.webview.postMessage({type: 'place', kind: 2, id: 'hphx', player: 0, x: 0, y: 0})
            await until(() => we.state?.undo, 'placed')
            we.focused = undefined
            await we.panel.webview.postMessage({type: 'focusEffect', path: 'Phoenix', distance: 6})
            const phoenix = await until(() => we.focused !== undefined && we.focused, 'focused')
            check(/phoenix/i.test(phoenix || ''), `the camera over it: ${phoenix}`)
            await sleep(1500)
            await shot('world-editor-ribbons')
            await we.panel.webview.postMessage({type: 'unfocus'})
            we.state = undefined
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.state, 'undone')
        }},
        {name: 'things', editor: true, run: async () => {
            log('things placed: a footman placed, picked, turned, dragged, deleted and back, saved into the map')
            await we.panel.webview.postMessage({type: 'look', x: 40, y: 40, distance: 12, horizontal: 0, vertical: -1.5})
            await sleep(300)
            we.state = undefined
            await we.panel.webview.postMessage({type: 'place', kind: 2, id: 'hfoo', player: 1, x: 0, y: 0})
            await until(() => we.state?.undo && we.state.dirty, 'placed')
            we.picked = undefined
            we.thing = undefined
            await we.panel.webview.postMessage({type: 'click', x: 0, y: 0})
            const footman = await until(() => we.picked !== undefined && we.picked, 'the footman picked')
            check(/^Footman \(hfoo\) · Player 2 · .* war3mapUnits\.doo #\d+/.test(footman), `picked: ${footman}`)
            await shot('world-editor-placed')
            log("the footman's properties: its owner, hit points and scale typed in the Select tool's panel")
            await we.panel.webview.postMessage({type: 'tool', tool: 'select'})
            const owned = await until(() => we.thing?.fields?.length && new Map(we.thing.fields), "the footman's fields")
            check(owned.get('player') === 1 && owned.get('hp') === -1 && owned.get('scale_x') === 1 && owned.has('mana') && !owned.has('level') && !owned.has('z'),
                `its fields: ${[...owned].join(' ')}`)
            for (const [field, value] of [['scale_x', 2], ['hp', 50], ['player', 3], ['hp', 0]]) {
                we.state = undefined
                await we.panel.webview.postMessage({type: 'thingField', field, value})
                await until(() => we.state, field)
                check(value ? !we.state.message : /hp can't be 0/.test(we.state.message), `${field} ${value}: ${we.state.message || 'set'}`)
            }
            await until(() => {
                const f = new Map(we.thing?.fields ?? [])
                return f.get('player') === 3 && f.get('hp') === 50 && f.get('scale_x') === 2
            }, 'the fields as typed')
            await shot('world-editor-thing')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => new Map(we.thing?.fields ?? []).get('player') === 1, 'the owner undone')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'redo'})
            await until(() => new Map(we.thing?.fields ?? []).get('player') === 3, 'and done again')
            // Its lists: an ability added, an item set to drop, as typed in the panel.
            const texts = () => new Map(we.thing?.texts ?? [])
            check(texts().get('abilities') === '' && texts().get('drops') === '' && !texts().has('items'), `a unit's lists: ${[...texts()].map(t => t.join('=')).join(' ')}`)
            await we.panel.webview.postMessage({type: 'thingList', list: 'abilities:add', value: 'AHhb'})
            await until(() => texts().get('abilities') === 'AHhb:1:0', 'an ability added')
            await we.panel.webview.postMessage({type: 'thingList', list: 'drops:new', value: 'ratc'})
            await until(() => texts().get('drops') === 'ratc:100', 'a set to drop')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'thingList', list: 'drops:0:add', value: 'ckng'})
            const over = await until(() => we.state?.message && we.state, 'a second item typed')
            check(/over 100/.test(over.message) && texts().get('drops') === 'ratc:100', `two of 100% refused: ${over.message}`)
            await shot('world-editor-thing-lists')
            // A random unit placed beside it: from a table of two.
            const at = Number(/#(\d+)/.exec(footman)[1]) + 1
            we.state = undefined
            await we.panel.webview.postMessage({type: 'place', kind: 2, id: 'uDNR', player: 0, x: 0.1, y: 0})
            await until(() => we.state, 'a random unit placed')
            await we.panel.webview.postMessage({type: 'select', file: 1, index: at})
            await until(() => texts().get('random') === 'any:0', 'its randomness: any of level 0')
            await we.panel.webview.postMessage({type: 'thingList', list: 'random:mode', value: 'table'})
            await until(() => texts().get('random') === 'table:', 'from a table')
            await we.panel.webview.postMessage({type: 'thingList', list: 'random:add', value: 'hfoo'})
            await until(() => texts().get('random') === 'table:hfoo:100', 'a footman in it')
            check(texts().has('random_groups'), `random unit #${at}: ${texts().get('random')}`)
            await shot('world-editor-random-unit')
            await we.panel.webview.postMessage({type: 'undo'})
            await we.panel.webview.postMessage({type: 'undo'})
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => !we.thing, 'the random unit undone')
            await we.panel.webview.postMessage({type: 'select', file: 1, index: at - 1})
            await until(() => we.thing?.index === at - 1, 'the footman again')

            for (const m of [{type: 'rotate', delta: Math.PI / 2}, {type: 'drag', x: 0.2, y: 0}, {type: 'delete'}]) {
                we.state = undefined
                await we.panel.webview.postMessage(m)
                await until(() => we.state, m.type)
                check(!we.state.message, `${m.type}: done`)
            }
            we.state = undefined
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.state?.redo, 'the delete undone')

            const stamp1 = fs.statSync(map).mtimeMs
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const savedUnits = await until(() => we.state?.message && we.state, 'saved')
            check(!savedUnits.dirty && /^Saved .*map\.w3x.war3mapUnits\.doo$/.test(savedUnits.message), savedUnits.message)
            check(fs.statSync(map).mtimeMs > stamp1, 'the map on disk written')
        }},
        {name: 'palette', editor: true, run: async () => {
            log("the object palette's icons, asked for as they're shown")
            await we.panel.webview.postMessage({type: 'tool', tool: 'place'})
            const iconCount = await until(() => we.icons, 'icons')
            check(iconCount > 10, `${iconCount} icons`)
            await shot('world-editor-palette')
        }},
        {name: 'regions', editor: true, run: async () => {
            log('regions: one drawn, renamed; the pathing painted; both saved')
            await we.panel.webview.postMessage({type: 'tool', tool: 'regions'})
            we.state = undefined
            await we.panel.webview.postMessage({type: 'region', mode: 0, from: [-0.3, -0.3], to: [0.3, 0.3]})
            const made = await until(() => we.region?.name.startsWith('Region ') && we.region, 'a region drawn')
            check(made.bounds[2] > made.bounds[0] && made.bounds.every(v => v % 32 === 0), `${made.name}: ${made.bounds.join(', ')}, on the grid of 32`)
            await we.panel.webview.postMessage({type: 'regionRename', name: 'E2E Spawn'})
            await until(() => we.region?.name === 'E2E Spawn', 'renamed')
            await we.panel.webview.postMessage({type: 'brush', brush: {tool: 'pathingOn', size: 2, flags: 8}, points: [[0, 0]]})
            await until(() => we.state?.undo && we.state.dirty, 'the pathing painted')
            await shot('world-editor-regions')
            const stamp2 = fs.statSync(map).mtimeMs
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const savedOverlays = await until(() => we.state?.message && we.state, 'saved')
            check(!savedOverlays.dirty && savedOverlays.message.includes('war3map.w3r') && savedOverlays.message.includes('war3map.wpm'), savedOverlays.message)
            check(fs.statSync(map).mtimeMs > stamp2, 'the map on disk written')
        }},
        {name: 'cameras', editor: true, run: async () => {
            log('cameras: one made from the view, renamed, its target dragged, looked through, deleted and undone; saved; one at 45° looked through, its fields typed')
            await we.panel.webview.postMessage({type: 'tool', tool: 'cameras'})
            const camsAt = we.overlays.cameras
            we.camera = undefined
            await we.panel.webview.postMessage({type: 'camera', action: 'new'})
            const cam = await until(() => we.camera?.count === camsAt + 1 && we.camera, 'a camera made from the view')
            check(/^Camera \d{3}$/.test(cam.name) && cam.index === camsAt && cam.view[3] === 70 && cam.view[2] > 0, `${cam.name}: rotation, angle of attack, distance, field of view ${cam.view.join(', ')}`)
            await we.panel.webview.postMessage({type: 'camera', action: 'rename', name: 'E2E Intro'})
            await until(() => we.camera?.name === 'E2E Intro', 'renamed')
            await we.panel.webview.postMessage({type: 'camera', action: 'drag', x: 0.3, y: -0.2})
            const dragged = await until(() => we.camera && Math.hypot(we.camera.target[0] - cam.target[0], we.camera.target[1] - cam.target[1]) > 0.5 && we.camera, 'its target dragged')
            check(dragged.name === 'E2E Intro', `its target from ${cam.target.map(v => v.toFixed(1))} to ${dragged.target.map(v => v.toFixed(1))} (cells)`)
            we.camera = undefined
            await we.panel.webview.postMessage({type: 'camera', action: 'look'})
            const looked = await until(() => we.camera?.looking && we.camera, 'looked through')
            check(Math.hypot(looked.looking.position[0] - looked.target[0], looked.looking.position[1] - looked.target[1]) < 1e-3 && Math.abs(looked.looking.distance * 128 - looked.view[2]) < 1 && looked.looking.fov === looked.view[3], `the view at its target, ${Math.round(looked.looking.distance * 128)} away, its field of view ${looked.looking.fov}`)
            await shot('world-editor-cameras')
            await we.panel.webview.postMessage({type: 'camera', action: 'delete'})
            await until(() => we.overlays?.cameras === camsAt, 'deleted')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.overlays?.cameras === camsAt + 1, 'undone: back')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const savedCams = await until(() => we.state?.message && we.state, 'saved')
            check(!savedCams.dirty && savedCams.message.includes('war3map.w3c'), savedCams.message)
            // One made from a view at 45°: its field of view 45; looked through from 70, the view's 45.
            await we.panel.webview.postMessage({type: 'fov', fov: 45})
            await we.panel.webview.postMessage({type: 'camera', action: 'new'})
            const narrow = await until(() => we.camera?.count === camsAt + 2 && we.camera, 'a camera made at 45°')
            check(narrow.view[3] === 45, `${narrow.name}: field of view ${narrow.view[3]}`)
            await we.panel.webview.postMessage({type: 'fov', fov: 70})
            we.camera = undefined
            await we.panel.webview.postMessage({type: 'camera', action: 'look'})
            const lookedNarrow = await until(() => we.camera?.looking && we.camera, 'looked through it')
            check(lookedNarrow.looking.fov === 45, `the view's field of view from 70 to ${lookedNarrow.looking.fov}`)
            await we.panel.webview.postMessage({type: 'fov', fov: 70})
            // Its fields typed in the panel.
            // In the order of a camera's `view`: rotation, angle of attack, distance, field of view, height, roll.
            const fields = [['rotation', 45], ['angle_of_attack', 320], ['distance', 2000], ['field_of_view', 60], ['z_offset', -150], ['roll', 10]]
            for (const [field, value] of fields) await we.panel.webview.postMessage({type: 'camera', action: 'field', field, value})
            const typedCam = await until(() => we.camera?.view[5] === 10 && we.camera, 'its fields typed')
            check(typedCam.view.join() === fields.map(t => t[1]).join(), `rotation, angle of attack, distance, field of view, height, roll typed: ${typedCam.view.join(', ')}`)
            // Looked through: the view rolled as it is.
            we.camera = undefined
            await we.panel.webview.postMessage({type: 'camera', action: 'look'})
            const rolled = await until(() => we.camera?.looking && we.camera, 'looked through it rolled')
            check(Math.abs(rolled.looking.roll * 180 / Math.PI - 10) < 1e-3 && rolled.looking.fov === 60, `the view's roll ${(rolled.looking.roll * 180 / Math.PI).toFixed(2)}°, field of view ${rolled.looking.fov}`)
            await shot('world-editor-camera-rolled')
            // One made from the rolled view: its roll the view's; deleted, the rolled one picked again.
            await we.panel.webview.postMessage({type: 'camera', action: 'new'})
            const fromRolled = await until(() => we.camera?.count === camsAt + 3 && we.camera, 'a camera made from the rolled view')
            check(Math.abs(fromRolled.view[5] - 10) < 0.01 && fromRolled.view[3] === 60, `${fromRolled.name}: roll ${fromRolled.view[5]}, field of view ${fromRolled.view[3]}`)
            await we.panel.webview.postMessage({type: 'camera', action: 'delete'})
            await until(() => we.overlays?.cameras === camsAt + 2, 'deleted')
            await we.panel.webview.postMessage({type: 'camera', action: 'pick', index: camsAt + 1})
            // Set to a view of no roll at 70 (the view through it): its roll and field of view the view's, its height kept, the same eye.
            await we.panel.webview.postMessage({type: 'fov', fov: 70, roll: 0})
            await we.panel.webview.postMessage({type: 'camera', action: 'set'})
            const unrolled = await until(() => we.camera?.view[5] === 0 && we.camera, 'set to the view')
            check(unrolled.view[3] === 70 && unrolled.view[4] === -150 && Math.abs(unrolled.view[2] - 2000) < 1, `${unrolled.name}: roll ${unrolled.view[5]}, field of view ${unrolled.view[3]}, height ${unrolled.view[4]}, distance ${unrolled.view[2]}`)
            await we.panel.webview.postMessage({type: 'camera', action: 'delete'})
            await until(() => we.overlays?.cameras === camsAt + 1, 'deleted')
        }},
        {name: 'sounds', editor: true, run: async () => {
            log("sounds: one made of a game's file, its fields typed, a flag, played, deleted and undone; saved")
            await we.panel.webview.postMessage({type: 'tool', tool: 'sounds'})
            const listed = await until(() => we.sounds, 'the sounds listed')
            const at = listed.count
            await we.panel.webview.postMessage({type: 'sound', action: 'new', file: 'Sound\\Interface\\BattleNetTick.wav'})
            const made = await until(() => we.sounds?.count === at + 1 && we.sounds.picked && we.sounds, 'a sound made')
            check(made.picked.name.startsWith('gg_snd_BattleNetTick') && made.picked.flags === 6 && !we.state?.message, `${made.picked.name}: ${made.picked.file}, flags ${made.picked.flags}`)
            for (const [field, value, bit] of [['volume', 90], ['pitch', 1.5], ['channel', 8], ['flags', true, 1], ['name', 'gg_snd_E2ETick'], ['volume', 200]]) {
                we.state = undefined
                await we.panel.webview.postMessage({type: 'sound', action: 'field', field, value, bit})
                await until(() => we.state, `${field} typed`)
                check(field === 'volume' && value === 200 ? /volume can't be/.test(we.state.message) : !we.state.message, `${field} ${value}: ${we.state.message || 'set'}`)
            }
            const typed = await until(() => {
                const p = we.sounds?.picked
                return p && p.name === 'gg_snd_E2ETick' && p.numbers[2] === 90 && p.numbers[3] === 1.5 && p.numbers[6] === 8 && p.flags === 7 && p
            }, 'its fields as typed')
            check(true, `${typed.name}: volume ${typed.numbers[2]}, pitch ${typed.numbers[3]}, channel ${typed.numbers[6]}, flags ${typed.flags}`)
            await shot('world-editor-sounds')
            we.played = undefined
            await we.panel.webview.postMessage({type: 'sound', action: 'play'})
            const played = await until(() => we.played, 'played')
            check(played.bytes > 1000 && played.played && !played.error, `played ${played.file}: ${played.bytes} bytes${played.error ? `, ${played.error}` : ''}`)
            await we.panel.webview.postMessage({type: 'sound', action: 'delete'})
            await until(() => we.sounds?.count === at, 'deleted')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.sounds?.count === at + 1, 'undone: back')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const saved = await until(() => we.state?.message && we.state, 'saved')
            check(!saved.dirty && saved.message.includes('war3map.w3s'), saved.message)
        }},
        {name: 'objects', editor: true, run: async () => {
            log("the object editor: the units listed, the footman's fields by type, an ability picked, hit points typed and refused, one based on it made, deleted and back; a string of the map's; copy and paste; Blizzard's cooldown at level 2; saved")
            await we.panel.webview.postMessage({type: 'object', action: 'open'})
            const units = await until(() => we.objects?.kind === 0 && we.objects.count > 100 && we.objects, 'the units listed')
            check(true, `${units.count} units, ${units.changed.length} changed by the map, ${units.custom.length} its own`)
            const field = (id, level = 0) => we.objects?.picked?.fields.find(f => f[0] === id && f[1] === level)
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'hfoo'})
            const footman = await until(() => we.objects?.picked?.id === 'hfoo' && we.objects.picked, "the footman's fields")
            check(field('uhpm')?.[2] === '420' && footman.fields.length > 50, `${footman.name}: ${footman.fields.length} fields, hit points ${field('uhpm')?.[2]}`)
            check(!field('ustr'), 'a unit: no hero\'s strength')
            // Its fields by their types: choices, checkboxes, its abilities by name, its icon drawn.
            const typed = await until(() => we.objects?.controls?.icons > 0 && we.objects, 'its icon drawn')
            check(/Defend/.test(field('uabi')?.[4] ?? '') && typed.controls.selects > 3 && typed.controls.checkboxes > 5,
                `abilities ${field('uabi')?.[2]}: ${field('uabi')?.[4]}; ${typed.controls.selects} choices, ${typed.controls.checkboxes} checkboxes, ${typed.controls.icons} icon`)
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'ua1t', value: 'pierce'})
            await until(() => field('ua1t')?.[2] === 'pierce' && field('ua1t')[3], 'its attack type chosen')
            check(true, `attack type ${field('ua1t')[2]} (${field('ua1t')[5]})`)
            // An ability picked from the list: its id added, named.
            await we.panel.webview.postMessage({type: 'object', action: 'pickId', field: 'uabi', id: 'AHhb'})
            // (The map renames Holy Light: its name, not its id.)
            await until(() => field('uabi')?.[2].split(',').includes('AHhb') && field('uabi')[4].split(', ').length === 3 && !field('uabi')[4].includes('AHhb'), 'Holy Light picked')
            check(true, `abilities ${field('uabi')[2]}: ${field('uabi')[4]}`)
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'Hpal'})
            const paladin = await until(() => we.objects?.picked?.id === 'Hpal' && we.objects.picked, "the paladin's fields")
            check(!!field('ustr'), `${paladin.name}, a hero: ${paladin.fields.length} fields, strength ${field('ustr')?.[2]}`)
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'hfoo'})
            await until(() => we.objects?.picked?.id === 'hfoo', 'the footman again')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'uhpm', value: 500})
            await until(() => field('uhpm')?.[2] === '500' && field('uhpm')[3], 'its hit points typed')
            check(we.objects.changed.includes('hfoo') && !we.state?.message, 'hit points 500: changed')
            await shot('world-editor-objects')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'uhpm', value: 0})
            const refused = await until(() => we.state?.message && we.state, 'refused')
            check(/1 to \d+, not 0/.test(refused.message), refused.message)
            const customs = we.objects.custom.length
            await we.panel.webview.postMessage({type: 'object', action: 'new'})
            const made = await until(() => we.objects?.custom.length === customs + 1 && we.objects.picked?.base === 'hfoo' && we.objects.picked.id !== 'hfoo' && we.objects.picked, 'one based on it')
            check(/^h[0-9A-Z]{3}$/.test(made.id) && field('uhpm')?.[2] === '500', `${made.id}: based on hfoo, its hit points copied: ${field('uhpm')?.[2]}`)
            await we.panel.webview.postMessage({type: 'object', action: 'delete'})
            await until(() => we.objects?.custom.length === customs, 'deleted')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.objects?.custom.length === customs + 1, 'undone: back')
            // A unit of the map's named by a string of its war3map.wts: shown as its text, typed into the string.
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'h000'})
            const named = await until(() => we.objects?.picked?.id === 'h000' && we.objects.picked, 'h000')
            check(!/TRIGSTR_/.test(named.name) && !/TRIGSTR_/.test(field('unam')?.[2] ?? 'TRIGSTR_'), `h000: ${named.name}, its name field ${field('unam')?.[2]}`)
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'unam', value: 'E2E Elemental'})
            await until(() => we.objects?.picked?.name.startsWith('E2E Elemental') && field('unam')?.[2] === 'E2E Elemental', 'its string typed')
            check(true, 'its name typed: war3map.wts\'s string')
            // Copied and pasted: a new one, its base and changes h000's.
            const before = we.objects.custom.length
            await we.panel.webview.postMessage({type: 'object', action: 'copy'})
            await we.panel.webview.postMessage({type: 'object', action: 'paste'})
            const pasted = await until(() => we.objects?.custom.length === before + 1 && we.objects.picked?.id !== 'h000' && we.objects.picked?.base === 'hwat' && we.objects.picked, 'pasted')
            check(pasted.name.startsWith('E2E Elemental'), `pasted: ${pasted.id}, ${pasted.name}`)
            // Its name its own string: renamed, h000's stays.
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'unam', value: 'E2E Copy'})
            await until(() => we.objects?.picked?.name.startsWith('E2E Copy'), 'the copy renamed')
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'h000'})
            const kept = await until(() => we.objects?.picked?.id === 'h000' && we.objects.picked, 'h000 again')
            check(kept.name.startsWith('E2E Elemental'), `the copy's name its own: h000 still ${kept.name}`)
            await we.panel.webview.postMessage({type: 'object', action: 'kind', k: 4})
            await until(() => we.objects?.kind === 4 && we.objects.count > 300, 'the abilities listed')
            await we.panel.webview.postMessage({type: 'object', action: 'pick', id: 'AHbz'})
            const blizzard = await until(() => we.objects?.picked?.id === 'AHbz' && we.objects.picked, "Blizzard's fields")
            check(blizzard.levels === 3 && field('acdn', 2), `${blizzard.name}: ${blizzard.levels} levels, cooldown at 2: ${field('acdn', 2)?.[2]}`)
            await we.panel.webview.postMessage({type: 'object', action: 'field', field: 'acdn', level: 2, value: 9})
            await until(() => field('acdn', 2)?.[2] === '9' && field('acdn', 2)[3] && !field('acdn', 1)[3], 'its cooldown at level 2 typed')
            check(true, 'cooldown at level 2: 9, level 1 the game\'s')
            await shot('world-editor-objects-ability')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const saved = await until(() => we.state?.message && we.state, 'saved')
            check(!saved.dirty && ['war3map.w3u', 'war3map.w3a', 'war3map.wts'].every(f => saved.message.includes(f)), saved.message)
            await we.panel.webview.postMessage({type: 'object', action: 'close'})
        }},
        {name: 'imports', editor: true, run: async () => {
            log("the import manager: the map's imported files, a texture looked at; a file from disk imported, moved, another deleted and back; saved")
            await we.panel.webview.postMessage({type: 'import', action: 'open'})
            const listed = await until(() => we.imports?.files.length && we.imports, 'the imports listed')
            const at = listed.files.findIndex(f => /\.blp$/i.test(f[0]) && f[1])
            check(at >= 0, `${listed.files.length} imported files, ${listed.files.filter(f => !f[1]).length} the map hasn't`)
            await we.panel.webview.postMessage({type: 'import', action: 'pick', index: at})
            const seen = await until(() => we.imports?.preview?.path === listed.files[at][0] && we.imports.preview, 'a texture looked at')
            check(seen.what === 1 && seen.width > 0 && !seen.error, `${seen.path}: ${seen.size} bytes, ${seen.width} wide`)
            await shot('world-editor-imports')
            const count = listed.files.length
            const source = path.join(root, 'triggers', 'Setup.j')
            await we.panel.webview.postMessage({type: 'import', action: 'add', source})
            await until(() => we.imports?.files.length === count + 1, 'imported')
            check(we.imports.files[count][0] === 'war3mapImported\\Setup.j' && we.imports.files[count][2], `imported: ${we.imports.files[count][0]}, to be written`)
            await we.panel.webview.postMessage({type: 'import', action: 'pick', index: count})
            await until(() => we.imports?.picked === count, 'picked')
            await we.panel.webview.postMessage({type: 'import', action: 'move', path: 'Scripts\\Setup.j'})
            await until(() => we.imports?.files[count]?.[0] === 'Scripts\\Setup.j', 'moved')
            await we.panel.webview.postMessage({type: 'import', action: 'pick', index: 0})
            await we.panel.webview.postMessage({type: 'import', action: 'delete'})
            await until(() => we.imports?.files.length === count, 'deleted')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.imports?.files.length === count + 1, 'undone: back')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const saved = await until(() => we.state?.message && we.state, 'saved')
            check(!saved.dirty && saved.message.includes('war3map.imp') && saved.message.includes('Scripts\\Setup.j'), saved.message)
            await we.panel.webview.postMessage({type: 'import', action: 'close'})
        }},
        {name: 'info', editor: true, run: async () => {
            log("the map's info: shown, its author and a flag, a player's race, the weather set; its name (a string of the map's) set and undone; saved")
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'open'})
            const info = await until(() => we.info?.name && we.info, 'the info shown')
            const name = info.name
            const players = Object.keys(info).filter(k => /^player\.\d+\.name$/.test(k)).length
            check(!/TRIGSTR/.test(name) && players > 0, `"${name}" by ${info.author}, version ${info.version}, ${players} players, ${Object.keys(info).filter(k => /^force\.\d+\.name$/.test(k)).length} forces`)
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'author', value: 'E2E Author'})
            await until(() => we.info?.author === 'E2E Author', 'its author typed')
            const fog = (Number(info.flags) & 0x2000) !== 0
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'flags', value: !fog, bit: 0x2000})
            await until(() => ((Number(we.info?.flags) & 0x2000) !== 0) === !fog, 'its fog flag')
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'player.0.race', value: 2})
            await until(() => we.info?.['player.0.race'] === '2', 'a race chosen')
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'weather', value: 'RAhr'})
            await until(() => we.info?.weather === 'RAhr', 'the weather chosen')
            check(true, `author ${we.info.author}, terrain fog ${!fog}, player 1 an orc, weather ${we.info.weather}`)
            // A player and a force added, then taken out.
            const count = k => Object.keys(we.info ?? {}).filter(f => new RegExp(`^${k}\\.\\d+\\.name$`).test(f)).length
            const [ps, fs] = [count('player'), count('force')]
            const free = [...Array(12).keys()].find(n => !Object.entries(we.info).some(([k, v]) => /^player\.\d+\.number$/.test(k) && Number(v) === n))
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'player.add', value: free})
            await until(() => count('player') === ps + 1, 'a player added')
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'force.add'})
            await until(() => count('force') === fs + 1, 'a force added')
            const added = Object.keys(we.info).find(k => /^player\.\d+\.number$/.test(k) && Number(we.info[k]) === free).split('.')[1]
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: `player.${added}.remove`})
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: `force.${fs}.remove`})
            await until(() => count('player') === ps && count('force') === fs, 'both taken out')
            check(true, `Player ${free + 1} and a force added and taken out: ${ps} players, ${fs} forces again`)
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'set', field: 'name', value: 'E2E Map'})
            await until(() => we.map?.name === 'E2E Map' && we.panel.title.includes('E2E Map'), 'its name typed: the tab follows')
            await shot('world-editor-info')
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => we.map?.name === name, 'its name undone')
            check(true, `its name: E2E Map, then ${we.map.name} again`)
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const saved = await until(() => we.state?.message && we.state, 'saved')
            check(!saved.dirty && saved.message.includes('war3map.w3i'), saved.message)
            await we.panel.webview.postMessage({type: 'mapInfo', action: 'close'})
        }},
        {name: 'map', editor: true, run: async () => {
            log("the map's settings: its tiles reordered and undone, a tile's pathing and undone, four cells added on the right, saved")
            we.mapSettings = undefined
            await we.panel.webview.postMessage({type: 'mapOpen'})
            const ms = await until(() => we.mapSettings, "the map's settings")
            check(ms.mapTiles.length > 1 && ms.tiles > ms.mapTiles.length, `${ms.width - 1} × ${ms.height - 1} cells, margins ${ms.margins.join(', ')}, ${ms.mapTiles.length} tiles of the game's ${ms.tiles}`)
            await shot('world-editor-map')
            const swapped = [ms.mapTiles[1], ms.mapTiles[0], ...ms.mapTiles.slice(2)]
            const to = ms.mapTiles.map((_, i) => (i === 0 ? 1 : i === 1 ? 0 : i))
            we.mapSettings = undefined
            await we.panel.webview.postMessage({type: 'mapTiles', ids: swapped, to, tileset: ms.tileset})
            const set = await until(() => we.mapSettings, 'the tiles set')
            check(set.mapTiles[0] === ms.mapTiles[1] && we.state?.undo, `the tiles: ${set.mapTiles.join(', ')}`)
            we.mapSettings = undefined
            await we.panel.webview.postMessage({type: 'undo'})
            const back = await until(() => we.mapSettings, 'undone')
            check(back.mapTiles.join() === ms.mapTiles.join(), 'undone: the tiles as they were')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'tilePathing', index: 0, forbids: 2})
            const pathed = await until(() => we.state?.message && we.state, 'the tile pathed')
            check(pathed.undo && /pathing set on \d+ cells/.test(pathed.message), pathed.message)
            await we.panel.webview.postMessage({type: 'undo'})
            we.mapSettings = undefined
            we.state = undefined
            await we.panel.webview.postMessage({type: 'mapResize', sides: {left: 0, right: 4, bottom: 0, top: 0}})
            const grown = await until(() => we.mapSettings, 'resized')
            const resizedState = await until(() => we.state?.message && we.state, 'its state')
            check(grown.width === ms.width + 4 && grown.margins[1] === ms.margins[1] + 4 && !resizedState.undo, `${resizedState.message} margins ${grown.margins.join(', ')}`)
            check(we.map.width === ms.width + 4, `the page has it: ${we.map.width} points across`)
            await shot('world-editor-resized')
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const savedSize = await until(() => we.state?.message && we.state, 'saved')
            check(!savedSize.dirty && savedSize.message.includes('war3map.w3e') && savedSize.message.includes('war3map.w3i') && savedSize.message.includes('war3map.wpm'), savedSize.message)
        }},
        {name: 'w3e-text', editor: true, run: async () => {
            log("the terrain's text form in step: its header follows the tile setter, and the editor follows it; closed unsaved")
            const w3eDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(map, 'war3map.w3e')).with({scheme: api.OBJECTS_SCHEME}))
            await vscode.window.showTextDocument(w3eDoc, {viewColumn: vscode.ViewColumn.Beside, preserveFocus: true})
            await sleep(500)
            const groundLine = () => w3eDoc.getText().split('\n').find(l => l.startsWith('ground='))
            const firstGround = groundLine()
            we.mapSettings = undefined
            await we.panel.webview.postMessage({type: 'mapOpen'})
            const tiles = (await until(() => we.mapSettings, "the map's settings")).mapTiles
            await we.panel.webview.postMessage({type: 'mapTiles', ids: [tiles[1], tiles[0], ...tiles.slice(2)], to: tiles.map((_, i) => (i === 0 ? 1 : i === 1 ? 0 : i)), tileset: we.mapSettings.tileset})
            // Until it has them (the document can change more than once on the way),
            // and is dirty: VS Code says so a moment after the change, not with it.
            await until(() => groundLine().startsWith(`ground=${tiles[1]},${tiles[0]}`) && w3eDoc.isDirty, 'the document has the tiles set')
            check(groundLine().startsWith(`ground=${tiles[1]},${tiles[0]}`) && w3eDoc.isDirty, `the document follows: ${groundLine().slice(0, 40)}…`)
            await we.panel.webview.postMessage({type: 'undo'})
            await until(() => groundLine() === firstGround, 'the document undone with the editor')
            const offset = w3eDoc.getText().split('\n').find(l => l.startsWith('offset_x='))
            const moved = new vscode.WorkspaceEdit()
            const at3 = w3eDoc.getText().indexOf(offset)
            moved.replace(w3eDoc.uri, new vscode.Range(w3eDoc.positionAt(at3), w3eDoc.positionAt(at3 + offset.length)), 'offset_x=' + (Number(offset.split('=')[1]) - 128))
            we.state = undefined
            await vscode.workspace.applyEdit(moved)
            const followed = await until(() => we.state?.undo && we.state, 'the editor takes the typed header')
            check(followed.dirty, 'typed in the text: the editor follows (the corner moved)')
            await shot('world-editor-w3e-text')
            await vscode.window.showTextDocument(w3eDoc, {viewColumn: vscode.ViewColumn.Beside})
            await vscode.commands.executeCommand('workbench.action.files.revert')
            await until(() => !w3eDoc.isDirty, 'reverted')
            await vscode.commands.executeCommand('workbench.action.closeActiveEditor')
            check(!w3eDoc.isDirty, 'reverted (the editor takes the header back) and closed, unsaved')
        }},
        {name: 'w3r-text', editor: true, needs: ['regions'], run: async () => {
            log("a text form in step: war3map.w3r open as text follows the editor, and the editor it; saved by VS Code")
            const w3rDoc = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(map, 'war3map.w3r')).with({scheme: api.OBJECTS_SCHEME}))
            await vscode.window.showTextDocument(w3rDoc, {viewColumn: vscode.ViewColumn.Beside, preserveFocus: true})
            await sleep(500)
            await we.panel.webview.postMessage({type: 'regionRename', name: 'From The Editor'})
            await until(() => w3rDoc.getText().includes('[From The Editor]') && w3rDoc.isDirty, 'the document has the editor\'s name')
            check(w3rDoc.isDirty, 'the document has it, not saved')
            const typed = new vscode.WorkspaceEdit()
            const at = w3rDoc.getText().indexOf('[From The Editor]')
            typed.replace(w3rDoc.uri, new vscode.Range(w3rDoc.positionAt(at), w3rDoc.positionAt(at + '[From The Editor]'.length)), '[Typed In Text]')
            await vscode.workspace.applyEdit(typed)
            await until(() => we.region?.name === 'Typed In Text', 'the editor has the typed name')
            check(true, 'typed in the text: the editor follows')
            await shot('world-editor-text')
            await we.panel.webview.postMessage({type: 'save'})
            await until(() => !w3rDoc.isDirty, 'the document saved by VS Code')
            check(!w3rDoc.isDirty, 'saved: the document')
        }},
        {name: 'cut', editor: true, run: async () => {
            log('the map cut: the cameras looking off it removed, said, saved')
            // The text forms closed first: the cut is what's looked at.
            const textTabs = vscode.window.tabGroups.all.flatMap(g => g.tabs).filter(t => t.input?.uri && /war3map\.(w3e|w3r)$/i.test(t.input.uri.path) && t.input.uri.scheme === api.OBJECTS_SCHEME)
            await vscode.window.tabGroups.close(textTabs)
            await sleep(500)
            // Cut on the left (last: what's off it is gone): the cameras looking off it removed, said, saved.
            const camsBefore = we.overlays.cameras
            we.overlays = undefined
            we.state = undefined
            await we.panel.webview.postMessage({type: 'mapResize', sides: {left: -40, right: 0, bottom: 0, top: 0}})
            // The terrain's and the regions' text forms are open: they follow first.
            const cutState = await until(() => we.state?.message && we.state, 'cut', 240000)
            const cutOv = await until(() => we.overlays, 'the overlays after the cut', 240000)
            const camsCut = Number((/(\d+) cameras/.exec(cutState.message) || [0, 0])[1])
            check(cutOv.cameras === camsBefore - camsCut, `${cutState.message} (cameras ${camsBefore} → ${cutOv.cameras})`)
            we.state = undefined
            await we.panel.webview.postMessage({type: 'save'})
            const savedCut = await until(() => we.state?.message && we.state, 'the cut saved', 240000)
            check(!savedCut.dirty && (camsCut === 0 || savedCut.message.includes('war3map.w3c')), savedCut.message)
        }},
        {name: 'triggers', run: async () => {
            log('the custom script opened as the Explorer opens it: the triggers come up')
            await vscode.commands.executeCommand('vscode.open', vscode.Uri.file(path.join(map, 'war3map.wct')).with({scheme: 'wc3map'}))
            const editor = await until(() => [...api.triggerEditors.values()][0], 'a trigger editor')
            const tree = await until(() => editor.state().tree, 'the tree')
            check(tree.triggers.length > 0, `${tree.triggers.length} triggers in the tree`)
            check(tree.wct && tree.wct.problem === null, 'the custom script goes with the triggers')
            const wct = editor.wct()
            check(!!wct, 'war3map.wct open beside, as a document')
            await shot('triggers')

            log('the line beside the tree dragged as a mouse does: the tree follows it, its width kept; a double click: back')
            await editor.page({type: 'e2eSplitter', dx: 120})
            const sp = await until(() => editor.state().splitter, "the page's answer")
            check(!sp.error, sp.error || 'the tree and the line are there')
            const near = (a, b) => Math.abs(a - b) <= 1
            const want = Math.min(sp.before + 120, sp.max)
            check(near(sp.midway, Math.min(sp.before + 60, sp.max)), `halfway: ${sp.before} -> ${sp.midway}`)
            check(near(sp.dragged, want), `dragged 120: ${sp.before} -> ${sp.dragged} (at most ${sp.max})`)
            check(sp.keptDragged === sp.dragged, `the width kept with the page: ${sp.keptDragged}`)
            check(near(sp.reset, sp.before) && sp.keptReset === null, `double click: ${sp.reset}, nothing kept`)

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
            check(errors.length === 0, "and no error at all: what the map's files place (gg_unit_, gg_rct_ ...) is known too")

            log("Rename on a variable's udg_ in the custom script: refused, World Editor's script makes it of the triggers")
            const heroes = wct.getText().indexOf('udg_Heroes')
            check(heroes >= 0, 'the custom script uses udg_Heroes')
            const refused = await vscode.commands.executeCommand('vscode.executeDocumentRenameProvider', wct.uri, wct.positionAt(heroes + 2), 'udg_Champions').then(() => null, e => String(e?.message ?? e))
            check(refused && refused.includes('World Editor makes from the map'), `refused: ${refused}`)

            const nth = tree.triggers.findIndex((t, k) => t.custom && !tree.wct.code[k][2]) + 1
            const name = tree.triggers[nth - 1].name
            log(`a trigger of custom text picked: ${nth} ${name}`)
            await editor.receive({type: 'pick', nth})
            let piece = await until(() => vscode.window.visibleTextEditors.find(e => e.document.uri.scheme === api.PIECE_SCHEME)?.document, 'its code opened')
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
            edit.insert(piece.uri, new vscode.Position(piece.lineCount, 0), '\nfunction E2eProbe takes nothing returns nothing\n    call NoSuchE2eFunction()\n    call RemoveUnit(gg_unit_e001_9999)\n    call RemoveUnit(gg_unit_n007_0225)\n    call RemoveItem(gg_item_n007_0225)\nendfunction')
            await vscode.workspace.applyEdit(edit)
            const diags = await until(() => vscode.languages.getDiagnostics(piece.uri).filter(d => d.message.includes('NoSuchE2eFunction')).length && vscode.languages.getDiagnostics(piece.uri), 'a diagnostic in the piece', 30000)
            const d = diags.find(d => d.message.includes('NoSuchE2eFunction'))
            check(piece.lineAt(d.range.start.line).text.includes('NoSuchE2eFunction'), `on its line in the piece (${d.range.start.line + 1}): ${d.message}`)
            check(diags.some(d => d.message.includes("'gg_unit_e001_9999'")), "a placed unit the map hasn't: unknown too")
            check(!diags.some(d => d.message.includes("'gg_unit_n007_0225'")) && diags.some(d => d.message.includes("'gg_item_n007_0225'")), "a placed unit is gg_unit_, not gg_item_: as the game's tables and the map's object data say")
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

            log('the trigger renamed: its names in its code follow, and its piece open with it')
            await editor.receive({type: 'field', key: 'name', value: 'Setup Two'})
            await editor.receive({type: 'renamed', from: name})
            await until(() => wct.getText().includes('function InitTrig_Setup_Two') && wct.getText().includes('gg_trg_Setup_Two'), 'InitTrig_ and gg_trg_ renamed in war3map.wct')
            check(!/\bInitTrig_Setup\b|\bgg_trg_Setup\b/.test(wct.getText()), 'no old name left in war3map.wct')
            piece = await until(() => vscode.window.visibleTextEditors.find(e => e.document.uri.fsPath.endsWith('Setup Two.j'))?.document, 'its piece reopened as Setup Two.j')
            check(piece.getText().startsWith('// e2e\n'), 'the piece reopened shows its code')
            check(!vscode.window.tabGroups.all.flatMap(g => g.tabs).some(t => t.input?.uri?.fsPath?.endsWith('/Setup.j') && t.input.uri.scheme === api.PIECE_SCHEME), 'the old piece closed')
            await shot('renamed')

            log('a trigger file of custom text: Open Code opens the .j beside it')
            const trg = vscode.Uri.file(path.join(path.dirname(map), 'triggers', 'Setup.trg'))
            await vscode.commands.executeCommand('vscode.openWith', trg, 'wc3.trigger')
            const trgEditor = await until(() => api.triggerEditors.get(trg.toString()), 'the trigger editor of Setup.trg')
            await trgEditor.receive({type: 'openCode', nth: 0, picked: true})
            const code = await until(() => vscode.window.visibleTextEditors.find(e => e.document.uri.fsPath.endsWith('Setup.j'))?.document, 'Setup.j open')
            check(code.getText().startsWith('// @dialect vjass'), 'Setup.j opened beside')
            await shot('trg-code')

            log("a GUI trigger of the map's: Show JASS, Show Lua, Show AngelScript, each beside, from its icon on the bar")
            const gui = tree.triggers.findIndex(t => !t.custom) + 1
            check(gui > 0, `trigger ${gui} (${tree.triggers[gui - 1]?.name}) is GUI`)
            await editor.receive({type: 'pick', nth: gui})
            const shownOf = {jass: ['wc3jass', '.j'], lua: ['wc3lua', '.lua'], angelscript: ['wc3as', '.as']}
            for (const [language, [scheme, ext]] of Object.entries(shownOf)) {
                await editor.receive({type: 'show', language})
                const doc = await until(() => vscode.workspace.textDocuments.find(d => d.uri.scheme === scheme && d.uri.path.endsWith(`war3map.wtg${ext}`) && d.getText().length > 0), `its ${language} open`)
                const text = doc.getText()
                check(doc.languageId === (language === 'jass' ? 'jass' : language), `war3map.wtg${ext} is ${doc.languageId}`)
                check(!/did not answer|can't be built/.test(text), `its ${language} is made${/did not answer|can't be built/.test(text) ? `: ${text.slice(0, 300)}` : ''}`)
                check(/gg_trg_\w+ = /.test(text) && /CreateTrigger\(\)/.test(text), `${language}: its trigger made in it${/gg_trg_/.test(text) ? '' : `:\n${text.slice(0, 600)}`}`)
                if (language === 'angelscript') check(/Jass::CreateTrigger\(\)/.test(text), "AngelScript: common.j's natives in Jass::")
                if (language === 'lua') check(text.startsWith('-- Made by wc3-build'), 'Lua: made by wc3-build')
            }
            await shot('show-languages')

            log("an object's id shows as its object's name, and its parameter's dialog offers the objects: the game's and the map's")
            const assault = editor.state().tree.triggers.findIndex(t => t.name === 'Main Chamber Assault') + 1
            check(assault > 0, `trigger ${assault} is Main Chamber Assault`)
            await editor.receive({type: 'pick', nth: assault})
            const seen = await until(() => editor.state().picked === assault && editor.state().tree?.view && editor.state().tree, 'its view')
            const allNodes = nodes => nodes.flatMap(n => [n, ...n.blocks.flatMap(b => allNodes(b.nodes))])
            const flat = spans => spans.map(x => typeof x === 'string' ? x : flat(x.s)).join('')
            const create = allNodes(seen.view.actions).find(n => n.name === 'CreateNUnitsAtLoc')
            check(!!create, 'it creates units')
            const unit = flat(create.text.find(x => typeof x !== 'string' && x.p === '1').s)
            await editor.receive({type: 'choices', line: create.line, path: '1'})
            const offered = await until(() => editor.state().choices?.objects?.length && editor.state().choices, "the parameter's choices")
            const id = offered.current.value
            const named = id => offered.objects.find(o => o[0] === id)?.[1]
            check(/^[a-zA-Z0-9]{4}$/.test(id) && unit === named(id) && unit !== id, `its unit ${id} shows as "${unit}", its name`)
            check(named('n00Q') && named('n00Q') !== 'n00Q' && named('hfoo') === 'Footman', `${offered.objects.length} units offered: the map's n00Q (${named('n00Q')}), the game's hfoo (${named('hfoo')})`)
            await shot('object-picker')
            await editor.receive({type: 'pick', nth: gui})

            log("Convert to Custom Text of that GUI trigger: custom text, its code in war3map.wct")
            const convertedName = editor.state().tree.triggers[gui - 1].name
            await editor.receive({type: 'convert', confirmed: true})
            const converted = await until(() => editor.state().tree?.triggers[gui - 1]?.custom, 'the trigger custom text')
            check(converted, `${convertedName} is custom text`)
            const ident = convertedName.replace(/[^A-Za-z0-9_]/g, '_').replace(/_$/, '_u')
            check(new RegExp(`function InitTrig_${ident} takes nothing returns nothing\\n\\s*set gg_trg_${ident} = CreateTrigger\\(\\)`).test(wct.getText()), `war3map.wct has its InitTrig_${ident}`)
            await shot('converted')

            log('saved from the piece: the map with both')
            const stamp = fs.statSync(map).mtimeMs
            // Typed in, then saved (a piece just opened is clean: nothing to save).
            edit = new vscode.WorkspaceEdit()
            edit.insert(piece.uri, new vscode.Position(0, 0), '// saved\n')
            await vscode.workspace.applyEdit(edit)
            await until(() => wct.getText().includes('// saved\n'), 'the line in war3map.wct')
            await piece.save()
            await until(() => !wct.isDirty, 'war3map.wct saved')
            const triggers = vscode.workspace.textDocuments.find(x => /war3map\.wtg$/i.test(x.uri.path) && x.uri.scheme === api.OBJECTS_SCHEME)
            await until(() => triggers && !triggers.isDirty, 'war3map.wtg saved with it')
            check(fs.statSync(map).mtimeMs > stamp, 'the map on disk was written')
            await shot('saved')
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
