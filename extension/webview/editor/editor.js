/* global acquireVsCodeApi, EditorProtocol, EditorCamera, EditorTerrain, EditorObjects, EditorOverlays, EditorLight, EditorMapSettings */

/**
 * The World Editor's page: it draws what the editor's process
 * (`wc3 editor`) sends and sends back what the user does; it never works
 * the map out itself. Frames come from the extension as `{type: 'frame',
 * kind, body}` and go to it as `{type: 'frame', data}` (see
 * `world-editor.js`); `{type: 'map'}` and `{type: 'error'}` tell the
 * extension what the page shows (its title, the tests); `{type: 'look'}`
 * from it points the camera.
 */
(() => {
    'use strict'
    const vscode = acquireVsCodeApi()
    const {encode, decode} = EditorProtocol
    const $ = id => document.getElementById(id)
    const send = data => vscode.postMessage({type: 'frame', data})

    const canvas = $('view')
    const gl = canvas.getContext('webgl2', {antialias: true})
    let terrain = null
    let objects = null
    let lines = null
    let shaderError = ''
    try {
        terrain = gl && new EditorTerrain.Terrain(gl)
        objects = gl && new EditorObjects.Objects(gl)
        lines = gl && new EditorOverlays.Lines(gl)
    } catch (e) {
        shaderError = e.message
    }
    const light = new EditorLight.Light()
    /** Where the camera was before `focusEffect`. */
    let focusedFrom = null
    let lightNotes = []
    const camera = new EditorCamera.Camera()
    camera.heightAt = (x, y) => terrain?.heightAt(x, y) ?? 0
    let placed = false
    let mapNotes = []
    let drawnSaid = false
    let overlayNotes = []

    function draw(now) {
        const dpr = window.devicePixelRatio || 1
        const w = Math.max(1, Math.round(canvas.clientWidth * dpr))
        const h = Math.max(1, Math.round(canvas.clientHeight * dpr))
        if (canvas.width !== w || canvas.height !== h) {
            canvas.width = w
            canvas.height = h
        }
        if (!gl) return
        gl.viewport(0, 0, w, h)
        gl.clearColor(0.08, 0.09, 0.1, 1)
        gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT)
        if (terrain?.ready) {
            const mvp = camera.matrix(w / h)
            const hours = light.now(now / 1000, show.cycle)
            showHours(hours)
            terrain.light = light
            terrain.hours = hours
            terrain.eye = camera.eye()
            terrain.showShadows = show.shadows
            light.showFog = show.fog
            terrain.draw(mvp, now / 1000)
            const {forward} = camera.axes()
            const {right, up} = camera.basis()
            objects?.draw(mvp, now / 1000, {right, up, forward, view: camera.direction, eye: camera.eye()}, light, hours)
            lines?.draw(mvp, {...show, cameraPicked: show.cameras})
        }
    }

    // For the tests: the picture's blocks over a run of frames, and
    // the blinks among them (a block unlike the frames on both sides of
    // it, which are like each other).
    let flicker = null
    const BLOCK = 32
    function flickerFrame() {
        const w = canvas.width
        const h = canvas.height
        const px = new Uint8Array(w * h * 4)
        gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, px)
        const bw = Math.floor(w / BLOCK)
        const bh = Math.floor(h / BLOCK)
        const sums = new Float32Array(bw * bh)
        for (let y = 0; y < bh * BLOCK; y += 2) {
            for (let x = 0; x < bw * BLOCK; x += 2) {
                const i = (y * w + x) * 4
                sums[Math.floor(y / BLOCK) * bw + Math.floor(x / BLOCK)] += px[i] + px[i + 1] + px[i + 2]
            }
        }
        const n = (BLOCK / 2) * (BLOCK / 2) * 3
        flicker.frames.push(sums.map(v => v / n))
        // A block watched (for the tests): its region, three blocks wide,
        // every frame, rows top to bottom.
        if (flicker.watch) {
            const size = BLOCK * 3
            const x0 = Math.max(0, (flicker.watch.x - 1) * BLOCK)
            const y0 = Math.max(0, (flicker.watch.y - 1) * BLOCK)
            const crop = new Uint8Array(size * size * 4)
            for (let y = 0; y < size; y++) {
                const from = ((y0 + size - 1 - y) * w + x0) * 4
                crop.set(px.subarray(from, from + size * 4), y * size * 4)
            }
            flicker.crops.push(crop)
        }
        if (flicker.frames.length < flicker.want) return
        const f = flicker.frames
        let blinks = 0
        const where = []
        for (let i = 1; i + 1 < f.length; i++) {
            for (let b = 0; b < f[i].length; b++) {
                const around = Math.abs(f[i - 1][b] - f[i + 1][b])
                const jump = Math.min(Math.abs(f[i][b] - f[i - 1][b]), Math.abs(f[i][b] - f[i + 1][b]))
                if (jump > 25 && around < 6) {
                    blinks++
                    // Where, in the view's terms too (-1 to 1, y up): to click there.
                    const [bx, by] = [b % bw, Math.floor(b / bw)]
                    if (where.length < 10) where.push({frame: i, x: bx, y: by, jump: Math.round(jump), vx: ((bx + 0.5) * BLOCK / w) * 2 - 1, vy: ((by + 0.5) * BLOCK / h) * 2 - 1})
                }
            }
        }
        // The watched block's region at the first blink of it and the frames
        // on both sides.
        const at = flicker.watch && where.find(p => p.x === flicker.watch.x && p.y === flicker.watch.y)
        const crops = at ? [at.frame - 1, at.frame, at.frame + 1].map(i => Array.from(flicker.crops[i])) : undefined
        vscode.postMessage({type: 'flicker', flicker: {frames: f.length, blocks: f[0].length, blinks, where, crops, size: BLOCK * 3}})
        flicker = null
    }

    // Keys held move the camera a little every frame.
    const held = new Set()
    let last = performance.now()
    function frame(now) {
        const dt = Math.min(0.1, (now - last) / 1000)
        last = now
        const speed = 40 * dt
        const k = key => held.has(key)
        const dx = (k('ArrowRight') || k('KeyD') ? 1 : 0) - (k('ArrowLeft') || k('KeyA') ? 1 : 0)
        const dy = (k('ArrowDown') || k('KeyS') ? 1 : 0) - (k('ArrowUp') || k('KeyW') ? 1 : 0)
        if (dx || dy) camera.move(dx * speed, dy * speed)
        draw(now)
        if (flicker) flickerFrame()
        drawMinimap()
        requestAnimationFrame(frame)
    }
    requestAnimationFrame(frame)

    let drag = null
    canvas.addEventListener('contextmenu', e => e.preventDefault())
    // A click: what's under it, asked of the editor (it answers Picked);
    // with a tool, a stroke (below).
    canvas.addEventListener('click', e => {
        if (!terrain?.ready || brush.tool !== 'select') return
        const r = canvas.getBoundingClientRect()
        const nx = ((e.clientX - r.left) / r.width) * 2 - 1
        const ny = 1 - ((e.clientY - r.top) / r.height) * 2
        const {origin, dir} = camera.ray(nx, ny, r.width / r.height)
        send(encode.pick(origin, dir))
    })
    canvas.addEventListener('mousedown', e => {
        if (e.button === 2 || e.button === 1) {
            drag = {x: e.clientX, y: e.clientY, turn: e.button === 1 || e.ctrlKey}
        }
    })
    window.addEventListener('mouseup', () => { drag = null })
    window.addEventListener('mousemove', e => {
        if (!drag) return
        const dx = e.clientX - drag.x
        const dy = e.clientY - drag.y
        drag.x = e.clientX
        drag.y = e.clientY
        if (drag.turn || e.ctrlKey) camera.turn(dx, dy)
        else camera.move(-dx * 0.025, -dy * 0.025)
    })
    canvas.addEventListener('wheel', e => {
        e.preventDefault()
        camera.zoom(-e.deltaY)
    }, {passive: false})
    window.addEventListener('keydown', e => {
        // A pick on the map for a trigger's parameter: Esc gives it up.
        if (mapPick && e.code === 'Escape') {
            e.preventDefault()
            endMapPick(null)
            return
        }
        // The trigger editor's keys are its own (Delete, Ctrl+Z ... there
        // are a trigger's); Ctrl+S saves both.
        if ($('trg-root').contains(/** @type {Node} */ (e.target))) {
            if ((e.ctrlKey || e.metaKey) && e.code === 'KeyS') {
                e.preventDefault()
                save()
            }
            return
        }
        const mod = e.ctrlKey || e.metaKey
        if (mod && e.code === 'KeyZ') {
            e.preventDefault()
            send(e.shiftKey ? encode.redo() : encode.undo())
            return
        }
        if (mod && (e.code === 'KeyY' || e.code === 'KeyS')) {
            e.preventDefault()
            if (e.code === 'KeyS') save()
            else send(encode.redo())
            return
        }
        if (mod || e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return
        if (brush.tool === 'regions' && selectedRegion !== null && (e.code === 'Delete' || e.code === 'Backspace')) {
            $('region-delete').onclick()
            return
        }
        if (brush.tool === 'cameras' && selectedCamera !== null && (e.code === 'Delete' || e.code === 'Backspace')) {
            $('camera-delete').onclick()
            return
        }
        if (selected && (e.code === 'Delete' || e.code === 'Backspace')) {
            send(encode.delete(selected.file, selected.index))
            selected = null
            objects?.select(-1, -1)
            $('status').textContent = ''
            return
        }
        if (selected && (e.code === 'KeyQ' || e.code === 'KeyE')) {
            send(encode.rotate(selected.file, selected.index, (e.code === 'KeyQ' ? 1 : -1) * Math.PI / 12))
            return
        }
        const toggle = TOGGLES.find(t => t.code === e.code)
        if (toggle) {
            show[toggle.key] = !show[toggle.key]
            applyShow()
        } else if (e.code === 'KeyG' && terrain) terrain.grid = !terrain.grid
        else if (e.code === 'Home' && terrain?.terrain) camera.reset(terrain.terrain.width - 1, terrain.terrain.height - 1)
        else held.add(e.code)
    })
    window.addEventListener('keyup', e => held.delete(e.code))
    window.addEventListener('blur', () => held.clear())

    // The layers over the map: each a button and a key, what's shown kept
    // with the page's state.
    const TOGGLES = [
        {key: 'pathing', label: 'Pathing', code: 'KeyP'},
        {key: 'destructables', label: 'Destructables', code: 'KeyO', title: 'The destructables\' pathing over it, as the game lays it (not in war3map.wpm): blockers, a bridge\'s deck walked on'},
        {key: 'regions', label: 'Regions', code: 'KeyR'},
        {key: 'cameras', label: 'Cameras', code: 'KeyC'},
        {key: 'bounds', label: 'Bounds', code: 'KeyB'},
        {key: 'minimap', label: 'Minimap', code: 'KeyM'},
        {key: 'animate', label: 'Animate', code: 'KeyN'},
        {key: 'shadows', label: 'Shadows', code: 'KeyH'},
        {key: 'fog', label: 'Fog', code: 'KeyF'},
        {key: 'cycle', label: 'Day cycle', code: 'KeyT'},
    ]
    const show = {pathing: false, destructables: true, regions: false, cameras: false, bounds: true, minimap: true, animate: true, shadows: true, fog: false, cycle: false, ...(vscode.getState()?.show ?? {})}
    const toggles = $('toggles')
    function applyShow() {
        if (terrain) terrain.show = {pathing: show.pathing, regions: show.regions, destructables: show.destructables}
        if (objects) objects.animate = show.animate
        $('minimap').hidden = !show.minimap
        for (const b of toggles.children) b.classList.toggle('on', !!show[b.dataset.key])
        vscode.setState({...(vscode.getState() ?? {}), show})
    }
    for (const t of TOGGLES) {
        const b = document.createElement('button')
        b.className = 'toggle'
        b.dataset.key = t.key
        b.textContent = t.label
        b.title = `${t.title ?? t.label} (${t.code.slice(3)})`
        b.onclick = () => {
            show[t.key] = !show[t.key]
            applyShow()
        }
        toggles.append(b)
    }
    applyShow()

    // The minimap: the terrain from above (north up), what's placed on
    // it, the camera's place; a click there moves the camera.
    const minimap = $('minimap')
    let minimapImage = null
    function drawMinimap() {
        if (!minimapImage || minimap.hidden) return
        const c = minimap.getContext('2d')
        c.putImageData(minimapImage, 0, 0)
        const [x, y] = camera.position
        c.strokeStyle = '#fff'
        c.lineWidth = Math.max(1, minimap.width / 160)
        const r = Math.max(2, camera.distance * 0.6)
        c.strokeRect(x - r, minimap.height - 1 - y - r * 0.6, r * 2, r * 1.2)
    }
    minimap.addEventListener('click', e => {
        const b = minimap.getBoundingClientRect()
        const x = (e.clientX - b.left) / b.width * minimap.width
        const y = minimap.height - 1 - (e.clientY - b.top) / b.height * minimap.height
        camera.position = [x, y, camera.heightAt(x, y)]
    })

    // The terrain palette: a tool, its brush; the editor lays it where
    // the mouse's ray meets the ground, a step at a time while the left
    // button is held (the next once the last is answered).
    const TOOL_LIST = [
        {tool: 'select', label: 'Select'},
        {tool: 'raise', label: 'Raise'},
        {tool: 'lower', label: 'Lower'},
        {tool: 'plateau', label: 'Plateau'},
        {tool: 'smooth', label: 'Smooth'},
        {tool: 'texture', label: 'Texture'},
        {tool: 'cliffUp', label: 'Cliff +'},
        {tool: 'cliffDown', label: 'Cliff −'},
        {tool: 'ramp', label: 'Ramp'},
        {tool: 'water', label: 'Water'},
        {tool: 'dry', label: 'Dry'},
        {tool: 'place', label: 'Place'},
        {tool: 'regions', label: 'Regions'},
        {tool: 'cameras', label: 'Cameras'},
        {tool: 'sounds', label: 'Sounds'},
        {tool: 'pathingOn', label: 'Pathing +'},
        {tool: 'pathingOff', label: 'Pathing −'},
    ]
    /** The tools that lay no brush. */
    const NO_BRUSH = ['select', 'place', 'regions', 'cameras', 'sounds']
    const brush = {tool: 'select', size: 4, strength: 0.25, texture: '', cliff: 0, flags: 2, ...(vscode.getState()?.brush ?? {})}
    const keep = () => vscode.setState({...(vscode.getState() ?? {}), brush})
    const tools = $('tools')
    function showTools() {
        for (const b of tools.querySelectorAll('[data-tool]')) b.classList.toggle('on', b.dataset.tool === brush.tool)
        $('textures').hidden = brush.tool !== 'texture'
        $('cliff-types').hidden = !brush.tool.startsWith('cliff')
        $('objects-palette').hidden = brush.tool !== 'place'
        const pathing = brush.tool.startsWith('pathing')
        $('pathing-flags').hidden = !pathing
        $('region-panel').hidden = brush.tool !== 'regions'
        $('camera-panel').hidden = brush.tool !== 'cameras'
        $('thing-panel').hidden = brush.tool !== 'select'
        $('sound-panel').hidden = brush.tool !== 'sounds'
        // (Not before the page's state is all declared: its first showing.)
        if (usedByReady) refreshUsedBy()
        $('size-label').hidden = NO_BRUSH.includes(brush.tool)
        $('strength-label').hidden = NO_BRUSH.includes(brush.tool) || pathing || brush.tool === 'ramp'
        for (const c of $('pathing-flags').querySelectorAll('input')) c.checked = (brush.flags & Number(c.value)) !== 0
        // A tool of a layer shows the layer.
        if ((pathing && !show.pathing) || (brush.tool === 'regions' && !show.regions) || (brush.tool === 'cameras' && !show.cameras)) {
            if (pathing) show.pathing = true
            else if (brush.tool === 'regions') show.regions = true
            else show.cameras = true
            applyShow()
        }
        for (const b of $('textures').children) b.classList.toggle('on', b.dataset.id === brush.texture)
        $('size-value').textContent = brush.size
        $('strength-value').textContent = brush.strength.toFixed(2)
        canvas.style.cursor = brush.tool === 'select' ? '' : 'crosshair'
        if (NO_BRUSH.includes(brush.tool) && terrain) terrain.brush = [0, 0, 0]
    }
    for (const t of TOOL_LIST) {
        const b = document.createElement('button')
        b.dataset.tool = t.tool
        b.textContent = t.label
        b.onclick = () => {
            brush.tool = t.tool
            keep()
            showTools()
        }
        $('tool-buttons').append(b)
    }
    $('size').value = brush.size
    $('size').oninput = () => {
        brush.size = Number($('size').value)
        keep()
        showTools()
    }
    $('strength').value = brush.strength
    $('strength').oninput = () => {
        brush.strength = Number($('strength').value)
        keep()
        showTools()
    }
    $('cliff-types').onchange = () => {
        brush.cliff = Number($('cliff-types').value)
        keep()
    }
    for (const c of $('pathing-flags').querySelectorAll('input')) {
        c.onchange = () => {
            brush.flags = c.checked ? brush.flags | Number(c.value) : brush.flags & ~Number(c.value)
            keep()
        }
    }
    $('undo').onclick = () => send(encode.undo())
    $('redo').onclick = () => send(encode.redo())
    /** The map's files saved, and the triggers' documents (war3map.wtg, war3map.wct: VS Code's) with them. */
    function save() {
        send(encode.save())
        if (triggerEditor) vscode.postMessage({type: 'saveTriggers'})
    }
    $('save').onclick = save
    const mapPanel = new EditorMapSettings.Panel($('map-panel'), send, encode)
    $('map-open').onclick = () => (mapPanel.isOpen ? mapPanel.close() : mapPanel.open())
    const importsPanel = new EditorImports.Panel($('imports-panel'), send, encode, msg => vscode.postMessage(msg))
    // Its tech tree's and upgrades' objects named by the object editor's lists: units, items, abilities, upgrades.
    const INFO_KINDS = [0, 1, 4, 6]
    const infoPanel = new EditorMapInfo.Panel($('info-panel'), send, encode, info => vscode.postMessage({type: 'info', info}), id => {
        for (const k of INFO_KINDS) {
            const o = objectsPanel.lists.get(k)?.find(o => o.id === id)
            if (o) return o.name
        }
    })
    const openInfo = infoPanel.open.bind(infoPanel)
    infoPanel.open = () => {
        openInfo()
        for (const k of INFO_KINDS) if (!objectsPanel.lists.has(k)) send(encode.objectList(k))
    }
    // The map's triggers: the trigger editor itself (`webview/triggers.js`,
    // the code of its own tab) mounted in this panel the first time it's
    // opened, its logic the extension's (`openTriggers`, then `trg`
    // messages both ways); its state kept apart from this page's.
    let triggerEditor = null
    const triggersPanel = {
        get isOpen() {
            return !$('triggers-panel').hidden
        },
        open() {
            $('triggers-panel').hidden = false
            if (triggerEditor) return
            vscode.postMessage({type: 'openTriggers'})
            triggerEditor = TriggerEditor.mount($('trg-root'), {
                pickOnMap: type => new Promise(resolve => startMapPick(type, resolve)),
                showOnMap: name => {
                    triggersPanel.close()
                    send(encode.mapFind(name))
                },
                postMessage: m => vscode.postMessage({type: 'trg', msg: m}),
                getState: () => vscode.getState()?.triggers,
                setState: s => vscode.setState({...(vscode.getState() ?? {}), triggers: s}),
            })
        },
        close() {
            $('triggers-panel').hidden = true
        },
    }
    /**
     * What uses what's picked (a thing, a region, a camera, by its name in
     * the map's script: `ident` as World Editor makes it): the triggers
     * whose lines name it, asked of the extension (`usedBy`), a click
     * opening one in the Triggers panel.
     */
    const ident = name => {
        const out = [...new TextEncoder().encode(name)].map(b => /[A-Za-z0-9_]/.test(String.fromCharCode(b)) && b < 128 ? String.fromCharCode(b) : '_').join('')
        return !out || out.endsWith('_') ? out + 'u' : out
    }
    let usedName = null
    function usedBy(name) {
        if (name === usedName) return
        usedName = name ?? null
        $('used-by').hidden = !usedName
        $('used-by').replaceChildren()
        if (usedName) vscode.postMessage({type: 'usedBy', name: usedName})
    }
    function showUsedBy(msg) {
        if (msg.name !== usedName) return
        const head = Object.assign(document.createElement('div'), {className: 'used-head', textContent: msg.uses.length ? `Used by triggers (${usedName})` : `No trigger uses ${usedName}`})
        $('used-by').replaceChildren(head, ...msg.uses.map(u => {
            const row = Object.assign(document.createElement('div'), {className: 'used-row', title: u.text})
            const where = u.nth === 0 ? 'Map script' : `${u.trigger || `Trigger ${u.nth}`}${u.custom ? ' (custom text)' : ''}`
            row.append(Object.assign(document.createElement('span'), {className: 'used-trigger', textContent: where}), Object.assign(document.createElement('span'), {className: 'used-text', textContent: u.text}))
            row.onclick = () => openTrigger(u.nth)
            return row
        }))
    }
    /** What uses what the tool shown has picked. */
    // eslint-disable-next-line no-var
    var usedByReady = false
    function refreshUsedBy() {
        if (brush.tool === 'select') usedBy(thing?.texts.find(([k]) => k === 'script_name')?.[1])
        else if (brush.tool === 'regions') usedBy(selectedRegion !== null && regions[selectedRegion] ? `gg_rct_${ident(regions[selectedRegion].name)}` : undefined)
        else if (brush.tool === 'cameras') usedBy(selectedCamera !== null && cameras[selectedCamera] ? `gg_cam_${ident(cameras[selectedCamera].name)}` : undefined)
        else usedBy(undefined)
    }
    /** The Triggers panel open on trigger `nth` (from 1). */
    function openTrigger(nth) {
        if (!triggersPanel.isOpen) toggle(triggersPanel)
        vscode.postMessage({type: 'trg', msg: {type: 'pick', nth}})
    }
    // A trigger's parameter picked on the map (`pickOnMap`): the panel
    // hidden, the layer of what's picked shown, a click asks the editor
    // the script's name of what it meets (`MapName`); Esc gives it up.
    let mapPick = null
    const PICKED = {unit: 'a unit', item: 'an item', destructable: 'a destructable', rect: 'a region', camerasetup: 'a camera'}
    function startMapPick(type, resolve) {
        mapPick = {type, resolve}
        triggersPanel.close()
        if (type === 'rect' && !show.regions) {
            show.regions = true
            applyShow()
        }
        if (type === 'camerasetup' && !show.cameras) {
            show.cameras = true
            applyShow()
        }
        $('status').textContent = `Click ${PICKED[type] ?? type} on the map for the trigger (Esc: none)`
        vscode.postMessage({type: 'mapPick', mapPick: {type, waiting: true}})
    }
    function endMapPick(name) {
        const p = mapPick
        mapPick = null
        $('status').textContent = ''
        toggle(triggersPanel)
        p?.resolve(name)
        vscode.postMessage({type: 'mapPick', mapPick: {type: p?.type, name}})
    }
    /** A click on the map while picking: what it meets asked for, by its point or its camera. */
    function mapPickAt(x, y) {
        const r = canvas.getBoundingClientRect()
        const index = mapPick.type === 'camerasetup' ? cameraAt(x - r.left, y - r.top) ?? -1 : -1
        const {origin, dir} = rayAt(x, y)
        send(encode.mapName(mapPick.type, index, origin, dir))
    }
    /**
     * What a name of the map's script is, found (`MapFound`): the view over
     * it, it picked with its tool (a thing with Select, a region with
     * Regions, a camera with Cameras).
     */
    function showFound(msg) {
        $('status').textContent = msg.what ? `${msg.name}: ${msg.said}` : msg.said
        vscode.postMessage({type: 'mapFound', mapFound: {name: msg.name, what: msg.what, index: msg.index, said: msg.said}})
        if (!msg.what || !terrain?.terrain) return
        const offset = terrain.terrain.offset
        const [x, y] = [(msg.at[0] - offset[0]) / 128, (msg.at[1] - offset[1]) / 128]
        camera.position = [x, y, msg.what === 1 ? msg.at[2] / 128 : camera.heightAt(x, y)]
        camera.distance = Math.min(camera.distance, 16)
        brush.tool = ['select', 'select', 'regions', 'cameras'][msg.what]
        showTools()
        if (msg.what === 1) {
            selected = {file: msg.file, index: msg.index}
            const found = objects?.find(msg.file, msg.index)
            objects?.select(found ? found.model : -1, found ? found.instance : -1)
            askThing()
        } else if (msg.what === 2) {
            selectedRegion = msg.index
            showRegion()
        } else {
            pickCamera(msg.index)
        }
    }
    canvas.addEventListener('click', e => {
        if (!mapPick) return
        e.stopImmediatePropagation()
        mapPickAt(e.clientX, e.clientY)
    }, true)
    // One of these panels at a time.
    const panels = () => [importsPanel, objectsPanel, infoPanel, triggersPanel]
    const toggle = panel => {
        if (panel.isOpen) return panel.close()
        for (const p of panels()) if (p !== panel) p.close()
        panel.open()
    }
    $('imports-open').onclick = () => toggle(importsPanel)
    $('info-open').onclick = () => toggle(infoPanel)
    $('triggers-open').onclick = () => toggle(triggersPanel)
    const objectsPanel = new EditorObjectEditor.Panel($('objects-panel'), send, encode, objects => vscode.postMessage({type: 'objects', objects}))
    $('objects-open').onclick = () => toggle(objectsPanel)
    $('reload').onclick = () => send(encode.reload())
    showTools()

    function showPalette(p) {
        const list = $('textures')
        list.replaceChildren(...p.grounds.map(g => {
            const b = document.createElement('button')
            b.className = 'texture'
            b.dataset.id = g.id
            b.title = `${g.name} (${g.id})`
            if (g.icon.width) {
                const c = document.createElement('canvas')
                c.width = g.icon.width
                c.height = g.icon.height
                c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(g.icon.rgba.buffer, g.icon.rgba.byteOffset, g.icon.rgba.byteLength), g.icon.width, g.icon.height), 0, 0)
                b.append(c)
            } else b.textContent = g.id
            b.onclick = () => {
                brush.texture = g.id
                keep()
                showTools()
            }
            return b
        }))
        if (!brush.texture && p.grounds[0]) brush.texture = p.grounds[0].id
        $('cliff-types').replaceChildren(...p.cliffs.map((c, i) => Object.assign(document.createElement('option'), {value: i, textContent: c.name})))
        $('cliff-types').value = String(Math.min(brush.cliff, Math.max(0, p.cliffs.length - 1)))
        showTools()
    }

    let stroke = null
    const rayAt = (x, y) => {
        const r = canvas.getBoundingClientRect()
        return camera.ray(((x - r.left) / r.width) * 2 - 1, 1 - ((y - r.top) / r.height) * 2, r.width / r.height)
    }
    const mouse = {x: 0, y: 0}
    /** A step of the stroke at the mouse, unless one is still unanswered. */
    function step() {
        if (!stroke || stroke.pending) return
        const {origin, dir} = rayAt(mouse.x, mouse.y)
        stroke.pending = true
        send(encode.brush(stroke.first, origin, dir, brush))
        stroke.first = false
    }
    canvas.addEventListener('mousedown', e => {
        if (e.button !== 0 || NO_BRUSH.includes(brush.tool) || !terrain?.ready) return
        stroke = {first: true, pending: false}
        Object.assign(mouse, {x: e.clientX, y: e.clientY})
        step()
    })
    window.addEventListener('mouseup', e => {
        if (e.button !== 0 || !stroke) return
        stroke = null
        send(encode.strokeEnd())
    })
    canvas.addEventListener('mousemove', e => {
        Object.assign(mouse, {x: e.clientX, y: e.clientY})
        if (thingDrag) return
        if (!NO_BRUSH.includes(brush.tool) && terrain?.ready) {
            const {origin, dir} = rayAt(e.clientX, e.clientY)
            const p = terrain.groundHit(origin, dir)
            terrain.brush = p ? [p[0], p[1], brush.size / 2] : [0, 0, 0]
        }
    })
    setInterval(step, 50)

    // What's placed: the object palette (a kind, a search, a type), the
    // thing picked; Place lays the type where a click meets the ground,
    // a drag of the thing picked moves it, Q and E turn it, Delete takes
    // it away.
    let selected = null
    let objectEntries = []
    const KINDS = ['Doodads', 'Destructables', 'Units', 'Items']
    const placing = {kind: 2, id: '', player: 0, filter: '', ...(vscode.getState()?.placing ?? {})}
    const keepPlacing = () => vscode.setState({...(vscode.getState() ?? {}), placing})
    for (const [k, label] of KINDS.entries()) {
        const b = document.createElement('button')
        b.textContent = label
        b.dataset.kind = k
        b.onclick = () => {
            placing.kind = k
            keepPlacing()
            showObjectList()
        }
        $('object-kinds').append(b)
    }
    $('object-search').value = placing.filter
    $('object-search').oninput = () => {
        placing.filter = $('object-search').value
        keepPlacing()
        showObjectList()
    }
    // Players 1 to 12, neutral hostile (12) and passive (15).
    $('object-player').replaceChildren(...[...Array(12).keys(), 12, 15].map(p => Object.assign(document.createElement('option'), {
        value: p,
        textContent: p < 12 ? `Player ${p + 1}` : p === 12 ? 'Neutral Hostile' : 'Neutral Passive',
    })))
    $('object-player').value = String(placing.player)
    $('object-player').onchange = () => {
        placing.player = Number($('object-player').value)
        keepPlacing()
    }
    function showObjectPalette(entries) {
        objectEntries = entries
        showObjectList()
    }
    function showObjectList() {
        for (const b of $('object-kinds').children) b.classList.toggle('on', Number(b.dataset.kind) === placing.kind)
        $('object-player').hidden = placing.kind !== 2
        const words = placing.filter.toLowerCase().split(/\s+/).filter(Boolean)
        const shown = objectEntries
            .filter(e => e.kind === placing.kind)
            .filter(e => words.every(w => `${e.name} ${e.id} ${e.group}`.toLowerCase().includes(w)))
            .slice(0, 300)
        icons.observer?.disconnect()
        $('object-list').replaceChildren(...shown.map(e => {
            const row = document.createElement('div')
            row.className = 'object' + (e.id === placing.id ? ' on' : '') + (e.custom ? ' custom' : '')
            row.dataset.key = `${e.kind}:${e.id}`
            const icon = document.createElement('canvas')
            icon.className = 'icon'
            icon.width = icon.height = 32
            row.append(icon, document.createTextNode(e.name))
            paintIcon(row)
            row.title = `${e.name} (${e.id})${e.group ? ' · ' + e.group : ''}${e.custom ? ' · the map\'s' : ''}`
            row.onclick = () => {
                placing.id = e.id
                keepPlacing()
                showObjectList()
            }
            return row
        }))
        // The icons of the rows shown, asked for as they come into view.
        icons.observer = new IntersectionObserver(seen => {
            for (const s of seen) if (s.isIntersecting) wantIcon(s.target.dataset.key)
        }, {root: $('object-list')})
        for (const row of $('object-list').children) icons.observer.observe(row)
    }

    // The palette's icons, asked of the editor a few at a time (it reads
    // each once), kept here: `null` for a type without one.
    const icons = {cache: new Map(), wanted: new Set(), timer: 0, observer: null}
    function wantIcon(key) {
        if (icons.cache.has(key) || icons.wanted.has(key)) return
        icons.wanted.add(key)
        icons.timer ||= setTimeout(() => {
            icons.timer = 0
            const batch = [...icons.wanted].slice(0, 64)
            batch.forEach(k => icons.wanted.delete(k))
            for (const k of batch) icons.cache.set(k, undefined)
            send(encode.icons(batch.map(k => {
                const at = k.indexOf(':')
                return [Number(k.slice(0, at)), k.slice(at + 1)]
            })))
            if (icons.wanted.size) wantIcon([...icons.wanted][0])
        }, 30)
    }
    function paintIcon(row) {
        const data = icons.cache.get(row.dataset.key)
        const canvas = row.querySelector('canvas')
        if (!data || !canvas) return
        canvas.width = data.width
        canvas.height = data.height
        canvas.getContext('2d').putImageData(data, 0, 0)
    }

    // Place: a click lays the type picked.
    canvas.addEventListener('click', e => {
        if (brush.tool !== 'place' || !placing.id || !terrain?.ready) return
        const {origin, dir} = rayAt(e.clientX, e.clientY)
        send(encode.place(placing.kind, placing.id, placing.player, origin, dir))
    })
    // Select: a drag of the thing picked moves it.
    let thingDrag = null
    canvas.addEventListener('mousedown', e => {
        if (e.button !== 0 || brush.tool !== 'select' || !selected) return
        thingDrag = {first: true, pending: false, moved: false, x: e.clientX, y: e.clientY}
    })
    canvas.addEventListener('mousemove', e => {
        if (!thingDrag || (Math.abs(e.clientX - thingDrag.x) + Math.abs(e.clientY - thingDrag.y) < 4 && !thingDrag.moved)) return
        thingDrag.moved = true
        if (thingDrag.pending) return
        const {origin, dir} = rayAt(e.clientX, e.clientY)
        thingDrag.pending = true
        send(encode.move(thingDrag.first, selected.file, selected.index, origin, dir))
        thingDrag.first = false
    })
    window.addEventListener('mouseup', () => {
        if (!thingDrag) return
        const moved = thingDrag.moved
        thingDrag = null
        if (moved) {
            send(encode.strokeEnd())
            suppressClick = true
        }
    })
    let suppressClick = false
    canvas.addEventListener('click', e => {
        if (suppressClick) {
            suppressClick = false
            e.stopImmediatePropagation()
        }
    }, true)

    // The thing picked's fields (the Select tool): those it has, as the
    // editor says (`thing`, asked when it's picked and after every edit),
    // each set as it's typed (in its range) by `ThingSet`.
    let thing = null
    const PLAYERS = [...[...Array(24).keys()].map(p => [p, `Player ${p + 1}`]), [24, 'Neutral Hostile'], [25, 'Neutral Victim'], [26, 'Neutral Extra'], [27, 'Neutral Passive']]
    const THING_FIELDS = [
        {field: 'x', label: 'X', step: 16},
        {field: 'y', label: 'Y', step: 16},
        {field: 'z', label: 'Z', step: 16},
        {field: 'facing', label: 'Facing', step: 15},
        {field: 'scale_x', label: 'Scale X', step: 0.1},
        {field: 'scale_y', label: 'Scale Y', step: 0.1},
        {field: 'scale_z', label: 'Scale Z', step: 0.1},
        {field: 'variation', label: 'Variation', step: 1},
        {field: 'state', label: 'State', options: [[2, 'Normal'], [1, 'Visible, walkable'], [0, 'Invisible, walkable']]},
        {field: 'life', label: 'Life %', step: 5},
        {field: 'player', label: 'Owner', options: PLAYERS},
        {field: 'hp', label: 'Hit points % (-1 its type\'s)', step: 5},
        {field: 'mana', label: 'Mana (-1 its type\'s)', step: 10},
        {field: 'level', label: 'Level', step: 1},
        {field: 'strength', label: 'Strength (0 its type\'s)', step: 1},
        {field: 'agility', label: 'Agility (0 its type\'s)', step: 1},
        {field: 'intelligence', label: 'Intelligence (0 its type\'s)', step: 1},
        {field: 'gold', label: 'Gold', step: 500},
        {field: 'target_acquisition', label: 'Acquisition (-1 normal, -2 camp)', step: 50},
        {field: 'color', label: 'Colour (-1 its owner\'s)', step: 1},
        {field: 'item_table', label: 'Item table (-1 none)', step: 1},
    ]
    for (const f of THING_FIELDS) {
        f.row = Object.assign(document.createElement('label'), {className: 'camera-field', textContent: f.label + ' ', hidden: true})
        if (f.options) {
            f.input = document.createElement('select')
            f.input.append(...f.options.map(([v, text]) => Object.assign(document.createElement('option'), {value: v, textContent: text})))
        } else {
            f.input = Object.assign(document.createElement('input'), {type: 'number', step: f.step})
        }
        f.input.dataset.field = f.field
        let timer = 0
        f.input.oninput = () => {
            clearTimeout(timer)
            const at = selected
            const v = Number(f.input.value)
            if (!at || f.input.value === '' || !Number.isFinite(v)) return
            timer = setTimeout(() => send(encode.thingSet(at.file, at.index, f.field, v)), 250)
        }
        f.row.append(f.input)
        $('thing-fields').append(f.row)
    }
    function askThing() {
        if (selected) send(encode.thingAsk(selected.file, selected.index))
        else showThing()
    }
    function showThing() {
        const t = thing && selected && thing.file === selected.file && thing.index === selected.index ? thing : null
        $('thing-panel').hidden = brush.tool !== 'select'
        $('thing-hint').hidden = !!t
        $('thing-hint').textContent = 'Click a thing placed'
        const values = new Map(t?.fields ?? [])
        for (const f of THING_FIELDS) {
            f.row.hidden = !values.has(f.field)
            if (!values.has(f.field) || document.activeElement === f.input) continue
            const v = Math.round(values.get(f.field) * 100) / 100
            if (f.options && ![...f.input.options].some(o => Number(o.value) === v)) {
                f.input.append(Object.assign(document.createElement('option'), {value: v, textContent: String(v)}))
            }
            f.input.value = String(v)
        }
        showThingLists(t)
        usedBy(brush.tool === 'select' ? t?.texts.find(([k]) => k === 'script_name')?.[1] : undefined)
        vscode.postMessage({type: 'thing', thing: t})
    }
    /**
     * Its lists (a hero's inventory, its abilities, its drops), named by
     * the object editor's lists (asked for once); kept while one of them
     * is typed in.
     */
    let listsAsked = false
    function showThingLists(t) {
        const box = $('thing-lists')
        if (box.contains(document.activeElement)) return
        if (t?.texts.length && !listsAsked) {
            listsAsked = true
            for (const k of EditorThingLists.KINDS) send(encode.objectList(k))
        }
        const at = selected
        EditorThingLists.render(box, t?.texts ?? [], (field, value) => at && send(encode.thingSetText(at.file, at.index, field, value)), (kind, id) => objectsPanel.lists.get(kind)?.find(o => o.id === id)?.name)
    }

    // Sounds (`war3map.w3s`, the Sounds tool): the list, New of a file,
    // the one picked's fields set as they're typed (`SoundSet`: empty is
    // "use default"), Play (its file's bytes asked of the editor, played
    // here), Delete.
    let sounds = []
    let selectedSound = null
    let selectNewSound = false
    /** `w3s_rs::Sound::numbers`' order: where each field is in a sound's `numbers`. */
    const SOUND_NUMBERS = ['fade_in', 'fade_out', 'volume', 'pitch', 'unknown_1', 'unknown_2', 'channel', 'min_distance', 'max_distance', 'distance_cutoff', 'cone_inside', 'cone_outside', 'cone_outside_volume', 'cone_x', 'cone_y', 'cone_z']
    /** What "use default" is stored as: -1, or 2^32. */
    const isDefault = v => v === -1 || v === 4294967296
    const CHANNELS = ['default', 'General', 'Unit Selection', 'Unit Acknowledgement', 'Unit Movement', 'Unit Ready', 'Combat', 'Error', 'Music', 'User Interface', 'Looping Movement', 'Looping Ambient', 'Animations', 'Construction', 'Birth', 'Fire']
    const SOUND_FLAGS = [[1, 'Looping'], [2, '3D sound'], [4, 'Stop when out of range'], [8, 'Music']]
    const SOUND_FIELDS = [
        {field: 'name', label: 'Variable', text: true},
        {field: 'file', label: 'File', text: true},
        {field: 'eax', label: 'EAX', text: true, list: ['DefaultEAXON', 'CombatSoundsEAX', 'KotoDrumsEAX', 'SpellsEAX', 'MissilesEAX', 'HeroAcksEAX', 'DoodadsEAX']},
        ...SOUND_FLAGS.map(([bit, label]) => ({field: 'flags', bit, label})),
        {field: 'channel', label: 'Channel', options: CHANNELS.map((name, i) => [i - 1, name])},
        {field: 'volume', label: 'Volume (0 to 127)', step: 1},
        {field: 'pitch', label: 'Pitch', step: 0.05},
        {field: 'fade_in', label: 'Fade in', step: 1},
        {field: 'fade_out', label: 'Fade out', step: 1},
        {field: 'min_distance', label: 'Min distance', step: 50},
        {field: 'max_distance', label: 'Max distance', step: 50},
        {field: 'distance_cutoff', label: 'Distance cutoff', step: 50},
        {field: 'cone_inside', label: 'Cone inside', step: 5},
        {field: 'cone_outside', label: 'Cone outside', step: 5},
        {field: 'cone_outside_volume', label: 'Cone outside volume', step: 1},
        {field: 'cone_x', label: 'Cone x', step: 0.1},
        {field: 'cone_y', label: 'Cone y', step: 0.1},
        {field: 'cone_z', label: 'Cone z', step: 0.1},
    ]
    for (const f of SOUND_FIELDS) {
        f.row = Object.assign(document.createElement('label'), {className: 'camera-field' + (f.text ? ' sound-text' : f.bit ? ' sound-flag' : ''), textContent: f.label + ' '})
        let timer = 0
        const sendLater = (value, ms) => {
            clearTimeout(timer)
            const index = selectedSound
            if (index === null) return
            timer = setTimeout(() => send(encode.soundSet(index, f.field, value)), ms)
        }
        if (f.bit) {
            f.input = Object.assign(document.createElement('input'), {type: 'checkbox'})
            f.input.onchange = () => {
                const s = sounds[selectedSound]
                if (s) sendLater((f.input.checked ? s.flags | f.bit : s.flags & ~f.bit) >>> 0, 0)
            }
            f.row.prepend(f.input)
        } else if (f.options) {
            f.input = document.createElement('select')
            f.input.append(...f.options.map(([v, text]) => Object.assign(document.createElement('option'), {value: v, textContent: text})))
            f.input.onchange = () => sendLater(f.input.value === '-1' ? '' : f.input.value, 0)
            f.row.append(f.input)
        } else {
            f.input = Object.assign(document.createElement('input'), f.text ? {type: 'text'} : {type: 'number', step: f.step, placeholder: 'default'})
            if (f.list) {
                const list = Object.assign(document.createElement('datalist'), {id: 'sound-eax-list'})
                list.append(...f.list.map(v => Object.assign(document.createElement('option'), {value: v})))
                f.row.append(list)
                f.input.setAttribute('list', list.id)
            }
            f.input.oninput = () => {
                if (f.text && !f.input.value.trim()) return
                sendLater(f.input.value, f.text ? 400 : 250)
            }
            f.row.append(f.input)
        }
        f.input.dataset.field = f.field
        $('sound-fields').append(f.row)
    }
    function showSounds() {
        const s = selectedSound !== null ? sounds[selectedSound] : null
        for (const id of ['sound-play', 'sound-delete']) $(id).disabled = !s
        for (const f of SOUND_FIELDS) {
            f.input.disabled = !s
            if (!s || document.activeElement === f.input) continue
            if (f.bit) f.input.checked = (s.flags & f.bit) !== 0
            else if (f.text) f.input.value = s[f.field]
            else {
                const v = s.numbers[SOUND_NUMBERS.indexOf(f.field)]
                f.input.value = isDefault(v) ? (f.options ? '-1' : '') : String(Math.round(v * 1000) / 1000)
            }
        }
        $('sound-hint').hidden = !!s
        $('sound-hint').textContent = sounds.length ? 'Pick a sound' : 'The map has no sounds'
        const list = $('sound-list')
        if (list.options.length !== sounds.length || sounds.some((x, i) => list.options[i].textContent !== x.name)) {
            list.replaceChildren(...sounds.map((x, i) => Object.assign(document.createElement('option'), {value: i, textContent: x.name, title: x.file})))
        }
        list.value = s ? String(selectedSound) : ''
        vscode.postMessage({type: 'sounds', sounds: {count: sounds.length, names: sounds.map(x => x.name), picked: s && {index: selectedSound, ...s}}})
    }
    $('sound-list').onchange = () => {
        selectedSound = $('sound-list').value === '' ? null : Number($('sound-list').value)
        showSounds()
    }
    $('sound-list').ondblclick = () => $('sound-play').onclick()
    $('sound-new').onclick = () => {
        const file = $('sound-new-file').value.trim()
        if (!file) return
        selectNewSound = true
        send(encode.soundNew(file))
    }
    // The game's and the map's sound files (asked for once), searched: a
    // click takes one as the new sound's file and plays it.
    let soundFiles = null
    const SHOWN = 300
    function showSoundFiles() {
        const words = $('sound-browse-search').value.toLowerCase().split(/\s+/).filter(Boolean)
        const found = (soundFiles ?? []).filter(f => words.every(w => f.toLowerCase().includes(w)))
        const rows = found.slice(0, SHOWN).map(f => {
            const row = Object.assign(document.createElement('div'), {className: 'obj-row', textContent: f, title: f})
            row.dataset.file = f
            row.onclick = () => {
                $('sound-new-file').value = f
                for (const r of $('sound-browse-list').children) r.classList.toggle('on', r === row)
                send(encode.soundPlay(f))
            }
            return row
        })
        if (found.length > SHOWN) rows.push(Object.assign(document.createElement('div'), {className: 'thing-sub', textContent: `… ${found.length - SHOWN} more: search`}))
        if (soundFiles === null) rows.push(Object.assign(document.createElement('div'), {className: 'thing-sub', textContent: 'Reading the files…'}))
        $('sound-browse-list').replaceChildren(...rows)
        vscode.postMessage({type: 'soundFiles', soundFiles: {count: soundFiles?.length ?? -1, shown: Math.min(found.length, SHOWN), first: found[0] ?? ''}})
    }
    $('sound-browse').onclick = () => {
        $('sound-browser').hidden = !$('sound-browser').hidden
        if (!$('sound-browser').hidden && soundFiles === null) send(encode.soundFiles())
        showSoundFiles()
    }
    $('sound-browse-search').oninput = showSoundFiles
    $('sound-delete').onclick = () => {
        if (selectedSound !== null) send(encode.soundDelete(selectedSound))
    }
    $('sound-play').onclick = () => {
        if (selectedSound !== null) send(encode.soundPlay(sounds[selectedSound].file))
    }
    /** The sound playing, stopped when another starts. */
    let playing = null
    async function playSound(msg) {
        const said = {file: msg.file, bytes: msg.bytes.length, error: msg.error, played: false}
        if (msg.bytes.length) {
            try {
                playing?.stop()
                const audio = new AudioContext()
                const buffer = await audio.decodeAudioData(msg.bytes.slice().buffer)
                playing = audio.createBufferSource()
                playing.buffer = buffer
                playing.connect(audio.destination)
                playing.start()
                said.played = true
            } catch (e) {
                said.error = `Can't play ${msg.file}: ${e.message || e}`
            }
        }
        if (said.error) $('status').textContent = said.error
        vscode.postMessage({type: 'played', played: said})
    }

    // Regions: a click picks one (the last drawn over the point); a drag
    // on the ground makes one, inside the one picked moves it, with Shift
    // moves its nearest corner. Its name is changed as it's typed.
    let regions = []
    let selectedRegion = null
    let regionDrag = null
    let renameTimer = 0
    const worldOf = p => terrain?.terrain ? [p[0] * 128 + terrain.terrain.offset[0], p[1] * 128 + terrain.terrain.offset[1]] : [0, 0]
    function showRegion() {
        const r = selectedRegion !== null ? regions[selectedRegion] : null
        if (brush.tool === 'regions') usedBy(r ? `gg_rct_${ident(r.name)}` : undefined)
        $('region-name').disabled = $('region-delete').disabled = !r
        if (document.activeElement !== $('region-name')) $('region-name').value = r ? r.name : ''
        $('region-bounds').textContent = r ? r.bounds.map(v => Math.round(v)).join(', ') : 'Click a region, or drag to draw one'
        vscode.postMessage({type: 'region', region: r && {name: r.name, bounds: r.bounds}})
    }
    $('region-name').oninput = () => {
        clearTimeout(renameTimer)
        const index = selectedRegion
        const name = $('region-name').value
        if (index === null || !name.trim()) return
        renameTimer = setTimeout(() => send(encode.regionRename(index, name)), 250)
    }
    $('region-delete').onclick = () => {
        if (selectedRegion === null) return
        send(encode.regionDelete(selectedRegion))
        selectedRegion = null
        showRegion()
    }
    canvas.addEventListener('mousedown', e => {
        if (e.button !== 0 || brush.tool !== 'regions' || !terrain?.ready) return
        const from = rayAt(e.clientX, e.clientY)
        const hit = terrain.groundHit(from.origin, from.dir)
        const r = selectedRegion !== null ? regions[selectedRegion] : null
        const [wx, wy] = hit ? worldOf(hit) : [NaN, NaN]
        const inside = r && wx >= r.bounds[0] && wx <= r.bounds[2] && wy >= r.bounds[1] && wy <= r.bounds[3]
        const mode = r && e.shiftKey ? 2 : inside ? 1 : 0
        regionDrag = {from, mode, index: mode ? selectedRegion : -1, first: true, pending: false, moved: false, x: e.clientX, y: e.clientY}
    })
    canvas.addEventListener('mousemove', e => {
        const d = regionDrag
        if (!d || d.pending || (!d.moved && Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) < 4)) return
        d.moved = true
        d.pending = true
        send(encode.region(d.first, d.mode, d.index, d.from, rayAt(e.clientX, e.clientY)))
        if (d.first && d.mode === 0) d.made = true
        d.first = false
    })
    window.addEventListener('mouseup', () => {
        const d = regionDrag
        if (!d) return
        regionDrag = null
        if (!d.moved) {
            // A click: what's there (Picked says the region).
            send(encode.pick(d.from.origin, d.from.dir))
            return
        }
        send(encode.strokeEnd())
        if (d.made) selectNewRegion = true
    })
    let selectNewRegion = false

    // Cameras: a click picks the one whose target (or eye) is nearest on
    // the screen, a drag from it moves its target over the ground; New
    // from view makes one looking as the view does, Set to view sets the
    // one picked so, Look through it the view as it looks.
    let cameras = []
    let selectedCamera = null
    let cameraDrag = null
    let cameraRenameTimer = 0
    let selectNewCamera = false
    /**
     * Its fields in the panel, each `at` its place in a camera's `view`
     * (`Overlays`'): each set as it's typed (in its range) by `CameraSet`.
     */
    const CAMERA_FIELDS = [
        {field: 'z_offset', label: 'Height', at: 4, min: -100000, max: 100000, step: 10},
        {field: 'rotation', label: 'Rotation', at: 0, min: 0, max: 360, step: 1},
        {field: 'angle_of_attack', label: 'Angle of attack', at: 1, min: 0, max: 360, step: 1},
        {field: 'distance', label: 'Distance', at: 2, min: 1, max: 100000, step: 50},
        {field: 'roll', label: 'Roll', at: 5, min: 0, max: 360, step: 1},
        {field: 'field_of_view', label: 'Field of view', at: 3, min: 1, max: 179, step: 1},
    ]
    for (const f of CAMERA_FIELDS) {
        const label = Object.assign(document.createElement('label'), {className: 'camera-field', textContent: f.label + ' '})
        f.input = Object.assign(document.createElement('input'), {type: 'number', min: f.min, max: f.max, step: f.step, disabled: true})
        f.input.dataset.field = f.field
        let timer = 0
        f.input.oninput = () => {
            clearTimeout(timer)
            const index = selectedCamera
            const v = Number(f.input.value)
            if (index === null || f.input.value === '' || !(v >= f.min && v <= f.max)) return
            timer = setTimeout(() => send(encode.cameraSet(index, f.field, v)), 250)
        }
        label.append(f.input)
        $('camera-fields').append(label)
    }
    const aspect = () => {
        const r = canvas.getBoundingClientRect()
        return r.width / r.height
    }
    /** Where a point (cells) is on the screen, in pixels from the canvas' top left; null behind the view. */
    function onScreen(p) {
        const m = camera.matrix(aspect())
        const x = m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12]
        const y = m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13]
        const w = m[3] * p[0] + m[7] * p[1] + m[11] * p[2] + m[15]
        if (w <= 0) return null
        const r = canvas.getBoundingClientRect()
        return [(x / w + 1) / 2 * r.width, (1 - y / w) / 2 * r.height]
    }
    /** The camera nearest the canvas' point (x, y), within 14 pixels; null none. */
    function cameraAt(x, y) {
        let best = null
        let bestD = 14
        cameras.forEach((c, i) => {
            for (const p of [c.target, c.eye]) {
                const s = onScreen(p)
                const d = s && Math.hypot(s[0] - x, s[1] - y)
                if (s && d < bestD) {
                    best = i
                    bestD = d
                }
            }
        })
        return best
    }
    function showCamera() {
        const c = selectedCamera !== null ? cameras[selectedCamera] : null
        if (brush.tool === 'cameras') usedBy(c ? `gg_cam_${ident(c.name)}` : undefined)
        for (const id of ['camera-name', 'camera-set', 'camera-look', 'camera-delete']) $(id).disabled = !c
        if (document.activeElement !== $('camera-name')) $('camera-name').value = c ? c.name : ''
        for (const f of CAMERA_FIELDS) {
            f.input.disabled = !c
            if (document.activeElement !== f.input) f.input.value = c ? String(Math.round(c.view[f.at] * 100) / 100) : ''
        }
        $('camera-hint').hidden = !!c
        $('camera-hint').textContent = 'Click a camera, or make one from the view'
        const list = $('camera-list')
        if (list.options.length !== cameras.length || cameras.some((c, i) => list.options[i].textContent !== c.name)) {
            list.replaceChildren(...cameras.map((c, i) => Object.assign(document.createElement('option'), {value: i, textContent: c.name})))
        }
        list.value = c ? String(selectedCamera) : ''
        // The one picked drawn white over the rest.
        const picked = []
        if (c) {
            // A little over the rest, to be drawn over them.
            const line = (a, b) => picked.push(a[0], a[1], a[2] + 0.02, 1, 1, 1, 1, b[0], b[1], b[2] + 0.02, 1, 1, 1, 1)
            line(c.eye, c.target)
            const [x, y, z] = c.target
            line([x - 0.8, y, z], [x + 0.8, y, z])
            line([x, y - 0.8, z], [x, y + 0.8, z])
        }
        lines?.set('cameraPicked', new Float32Array(picked))
        vscode.postMessage({type: 'camera', camera: c && {index: selectedCamera, name: c.name, target: c.target, view: c.view, count: cameras.length}})
    }
    function pickCamera(i) {
        selectedCamera = i
        showCamera()
    }
    $('camera-list').onchange = () => pickCamera($('camera-list').value === '' ? null : Number($('camera-list').value))
    $('camera-list').ondblclick = () => $('camera-look').onclick()
    $('camera-name').oninput = () => {
        clearTimeout(cameraRenameTimer)
        const index = selectedCamera
        const name = $('camera-name').value
        if (index === null || !name.trim()) return
        cameraRenameTimer = setTimeout(() => send(encode.cameraRename(index, name)), 250)
    }
    // The view's field of view (70, World Editor's, but through a camera) and roll.
    const fromView = index => send(encode.cameraView(index, camera.position, camera.distance, camera.horizontal, camera.vertical, camera.fov, camera.roll))
    $('camera-new').onclick = () => {
        selectNewCamera = true
        fromView(-1)
    }
    $('camera-set').onclick = () => selectedCamera !== null && fromView(selectedCamera)
    $('camera-look').onclick = () => {
        const c = selectedCamera !== null ? cameras[selectedCamera] : null
        if (!c) return
        const d = [0, 1, 2].map(k => c.target[k] - c.eye[k])
        const len = Math.hypot(...d) || 1
        camera.position = [...c.target]
        camera.distance = Math.max(0.5, len)
        camera.vertical = Math.max(-Math.PI / 2 + 0.001, Math.min(Math.PI / 2 - 0.001, Math.asin(d[2] / len)))
        camera.horizontal = Math.atan2(d[0], d[1])
        camera.fov = Math.max(1, Math.min(179, c.view[3]))
        camera.roll = c.view[5] * Math.PI / 180
    }
    $('camera-delete').onclick = () => {
        if (selectedCamera === null) return
        send(encode.cameraDelete(selectedCamera))
        selectedCamera = null
        showCamera()
    }
    canvas.addEventListener('mousedown', e => {
        if (e.button !== 0 || brush.tool !== 'cameras' || !terrain?.ready) return
        const r = canvas.getBoundingClientRect()
        pickCamera(cameraAt(e.clientX - r.left, e.clientY - r.top))
        if (selectedCamera !== null) cameraDrag = {index: selectedCamera, first: true, pending: false, moved: false, x: e.clientX, y: e.clientY}
    })
    canvas.addEventListener('mousemove', e => {
        const d = cameraDrag
        if (!d || d.pending || (!d.moved && Math.abs(e.clientX - d.x) + Math.abs(e.clientY - d.y) < 4)) return
        d.moved = true
        d.pending = true
        send(encode.camera(d.first, d.index, rayAt(e.clientX, e.clientY)))
        d.first = false
    })
    window.addEventListener('mouseup', () => {
        const d = cameraDrag
        cameraDrag = null
        if (d?.moved) send(encode.strokeEnd())
    })

    function showState(st) {
        $('undo').disabled = !st.undo
        $('redo').disabled = !st.redo
        $('save').disabled = !st.dirty
        $('name').classList.toggle('dirty', st.dirty)
        if (st.message) $('status').textContent = st.message
        vscode.postMessage({type: 'state', state: st})
    }

    // The time of day: the slider, or the day going round.
    $('hours').value = String(light.hours)
    $('hours').oninput = () => {
        light.hours = Number($('hours').value)
    }
    let shownHours = -1
    function showHours(hours) {
        const minutes = Math.floor(hours * 60)
        if (minutes === shownHours) return
        shownHours = minutes
        $('hours-value').textContent = `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`
        if (show.cycle) $('hours').value = String(hours)
    }

    function showNotes() {
        const all = [...mapNotes, ...(terrain?.notes ?? []), ...(terrain?.cliffNotes ?? []), ...(objects?.notes ?? []), ...overlayNotes, ...lightNotes]
        const notes = $('notes')
        notes.replaceChildren(...all.map(n => Object.assign(document.createElement('li'), {textContent: n})))
        notes.hidden = !all.length
    }

    /** A message over the view; `reopen` offers to start the editor again. */
    function say(title, text, reopen = false) {
        $('message').hidden = false
        $('message-title').textContent = title
        $('message-text').textContent = text
        $('reopen').hidden = !reopen
    }

    function showMap(m) {
        $('message').hidden = true
        $('name').textContent = m.name || m.project
        const cells = `${Math.max(0, m.width - 1)} × ${Math.max(0, m.height - 1)}`
        const tileset = m.tilesetName ? `${m.tilesetName} (${m.tileset})` : m.tileset
        $('meta').textContent = [cells, tileset, m.author && `by ${m.author}`].filter(Boolean).join(' · ')
        mapNotes = [...m.notes]
        showNotes()
        vscode.postMessage({type: 'map', map: {name: m.name, width: m.width, height: m.height, tileset: m.tileset}})
    }

    function onFrame(kind, body) {
        let msg
        try {
            msg = decode(kind, body)
        } catch (e) {
            return say('Can\'t read what the editor sent', e.message)
        }
        if (!msg) return
        if (msg.kind === 'map') showMap(msg)
        else if (msg.kind === 'ground' && terrain) {
            terrain.setGround(msg)
            for (const t of msg.textures) if (t.error) mapNotes.push(`Ground ${t.id}: ${t.error}`)
            showNotes()
        } else if (msg.kind === 'water' && terrain) {
            terrain.setWater(msg)
            if (msg.error) mapNotes.push(`Water: ${msg.error}`)
            showNotes()
        } else if (msg.kind === 'cliffs' && terrain) {
            terrain.setCliffs(msg)
            showNotes()
            // How many ramps stand (their models': `CliffTrans` ...), for the tests.
            const count = m => m.instances.length / 4
            vscode.postMessage({type: 'cliffs', cliffs: {
                instances: msg.meshes.reduce((n, m) => n + count(m), 0),
                ramps: msg.meshes.filter(m => /trans|ramp/i.test(m.path)).reduce((n, m) => n + count(m), 0),
                missing: msg.meshes.filter(m => m.error).length + (msg.notes?.length ?? 0),
            }})
        } else if (msg.kind === 'terrain' && terrain) {
            terrain.setTerrain(msg)
            // Resized: the view stays over the same ground.
            if (mapPanel.shift) {
                camera.position[0] += mapPanel.shift[0]
                camera.position[1] += mapPanel.shift[1]
                mapPanel.shift = null
            }
            if (!placed) {
                camera.reset(msg.width - 1, msg.height - 1)
                placed = true
            }
            showNotes()
        } else if (msg.kind === 'objects' && objects) {
            objects.set(msg)
            showNotes()
        } else if (msg.kind === 'instances' && objects) {
            objects.setInstances(msg.models)
            // What's picked, where it is now (an edit moves the instances).
            // (A thing without a model, a random unit, is still there: the
            // editor says when it isn't.)
            const found = selected && objects.find(selected.file, selected.index)
            objects.select(found ? found.model : -1, found ? found.instance : -1)
            askThing()
            if (!drawnSaid) {
                drawnSaid = true
                requestAnimationFrame(() => vscode.postMessage({
                    type: 'drawn',
                    terrain: {
                        width: terrain.terrain.width,
                        height: terrain.terrain.height,
                        layers: terrain.layers ? 1 : 0,
                        water: terrain.water?.count ?? 0,
                        cliffs: terrain.cliffs.reduce((n, c) => n + c.instances, 0),
                        objects: objects.count,
                        animated: objects.models.filter(m => m.anim).length,
                        billboards: objects.models.filter(m => m.billboards).length,
                        startLocations: objects.models.filter(m => /startlocation/i.test(m.path ?? '')).reduce((n, m) => n + m.instances, 0),
                        geosetAnimated: objects.models.filter(m => m.geosetAnimated).length,
                        emitters: objects.particles.count,
                        tails: objects.particles.models.flat().filter(x => x.e.params[14] !== 0).length,
                        modelSpace: objects.particles.models.flat().filter(x => (x.e.params[11] & 0x80000) !== 0).length,
                        ribbons: objects.particles.ribbonCount,
                        error: gl.getError(),
                    },
                }))
            }
        } else if (msg.kind === 'overlays' && terrain) {
            terrain.setOverlays(msg)
            regions = msg.regions
            if (selectNewRegion || (regionDrag?.made && selectedRegion === null)) {
                selectedRegion = regions.length - 1
                selectNewRegion = false
            }
            if (selectedRegion !== null && selectedRegion >= regions.length) selectedRegion = null
            showRegion()
            lines?.set('bounds', msg.bounds)
            lines?.set('cameras', msg.cameraLines)
            cameras = msg.cameras
            if (selectNewCamera) {
                selectedCamera = cameras.length - 1
                selectNewCamera = false
            }
            if (selectedCamera !== null && selectedCamera >= cameras.length) selectedCamera = null
            showCamera()
            if (msg.minimap.width) {
                minimap.width = msg.minimap.width
                minimap.height = msg.minimap.height
                minimapImage = new ImageData(new Uint8ClampedArray(msg.minimap.data.buffer, msg.minimap.data.byteOffset, msg.minimap.data.byteLength), msg.minimap.width, msg.minimap.height)
            }
            overlayNotes = msg.notes
            showNotes()
            vscode.postMessage({type: 'overlays', overlays: {
                pathing: msg.pathing.width,
                // The destructables' cells, and their walkable decks.
                destructables: msg.destructables.data.reduce((n, c) => n + ((c & 14) !== 0 ? 1 : 0), 0),
                decks: msg.destructables.data.reduce((n, c) => n + (c & 1), 0),
                regions: msg.regions.length,
                cameras: msg.cameras.length,
                bounds: msg.bounds.length / 7,
                minimap: msg.minimap.width,
                // The first region's name and middle (the game's units).
                region: msg.regions[0] && {name: msg.regions[0].name, x: (msg.regions[0].bounds[0] + msg.regions[0].bounds[2]) / 2, y: (msg.regions[0].bounds[1] + msg.regions[0].bounds[3]) / 2},
            }})
        } else if (msg.kind === 'palette') {
            showPalette(msg)
            mapPanel.setIcons(msg.grounds)
        } else if (msg.kind === 'mapSettings') {
            mapPanel.set(msg)
            vscode.postMessage({type: 'mapSettings', settings: {width: msg.width, height: msg.height, margins: msg.margins, tileset: msg.tileset, mapTiles: msg.mapTiles, tiles: msg.tiles.length}})
        } else if (msg.kind === 'state') {
            if (stroke) stroke.pending = false
            if (thingDrag) thingDrag.pending = false
            if (regionDrag) regionDrag.pending = false
            if (cameraDrag) cameraDrag.pending = false
            showState(msg)
            // A value refused: the thing's fields as they are again.
            if (msg.message && selected) askThing()
            // The object editor's list and fields as they are now (an undo
            // of a string of the map's changes no object data).
            if (objectsPanel.isOpen && !msg.message) send(encode.objectList(objectsPanel.kind))
        } else if (msg.kind === 'light' && terrain) {
            light.set(msg)
            terrain.setShadows(msg.shadows)
            lightNotes = msg.notes
            showNotes()
            vscode.postMessage({type: 'light', light: {day: msg.terrain.length > 0, shadows: msg.shadows.width, fog: !!msg.fog}})
        } else if (msg.kind === 'icons') {
            for (const i of msg.icons) {
                const key = `${i.kind}:${i.id}`
                const image = i.width ? new ImageData(new Uint8ClampedArray(i.rgba.buffer, i.rgba.byteOffset, i.rgba.byteLength), i.width, i.height) : null
                // A texture's by its path: the object editor's.
                if (i.kind === 255) {
                    objectsPanel.setIcon(i.id, image)
                    continue
                }
                icons.cache.set(key, image)
                const row = $('object-list').querySelector(`[data-key="${CSS.escape(key)}"]`)
                if (row) paintIcon(row)
            }
            vscode.postMessage({type: 'icons', icons: [...icons.cache.values()].filter(Boolean).length})
        } else if (msg.kind === 'objectPalette') {
            showObjectPalette(msg.entries)
        } else if (msg.kind === 'picked' && objects) {
            if (brush.tool === 'regions') {
                selectedRegion = msg.region
                showRegion()
                $('status').textContent = msg.text
                vscode.postMessage({type: 'picked', text: msg.text})
                return
            }
            selected = msg.found ? {file: msg.file, index: msg.index} : null
            objects.select(msg.found ? msg.model : -1, msg.found ? msg.instance : -1)
            askThing()
            $('status').textContent = msg.found ? msg.text : ''
            vscode.postMessage({type: 'picked', text: msg.found ? msg.text : ''})
        } else if (msg.kind === 'info') {
            infoPanel.set(msg.fields)
        } else if (msg.kind === 'imports') {
            importsPanel.set(msg.files)
        } else if (msg.kind === 'importPreview') {
            importsPanel.setPreview(msg)
        } else if (msg.kind === 'objectEnums') {
            objectsPanel.setEnums(msg.enums)
        } else if (msg.kind === 'objectList') {
            objectsPanel.setList(msg)
            if (EditorThingLists.KINDS.includes(msg.objectKind)) showThingLists(thing)
            if (infoPanel.isOpen && !$('info-panel').contains(document.activeElement)) infoPanel.render()
        } else if (msg.kind === 'objectFields') {
            objectsPanel.setFields(msg)
        } else if (msg.kind === 'sounds') {
            sounds = msg.sounds
            if (selectNewSound) {
                selectedSound = sounds.length ? sounds.length - 1 : null
                selectNewSound = false
            }
            if (selectedSound !== null && selectedSound >= sounds.length) selectedSound = null
            showSounds()
        } else if (msg.kind === 'mapFound') {
            showFound(msg)
        } else if (msg.kind === 'mapName') {
            if (!mapPick || msg.type !== mapPick.type) return
            if (msg.name) endMapPick(msg.name)
            else $('status').textContent = `${msg.said} (Esc: none)`
        } else if (msg.kind === 'soundFiles') {
            soundFiles = msg.files
            showSoundFiles()
        } else if (msg.kind === 'soundData') {
            playSound(msg)
        } else if (msg.kind === 'thing') {
            thing = msg.fields.length ? msg : null
            if (!thing && selected?.file === msg.file && selected.index === msg.index) selected = null
            showThing()
        } else if (msg.kind === 'error') {
            $('name').textContent = 'Can\'t open the map'
            say('Can\'t open the map', msg.message, true)
            vscode.postMessage({type: 'error', message: msg.message})
        }
    }

    usedByReady = true
    window.addEventListener('message', e => {
        const msg = e.data
        if (msg.type === 'frame') onFrame(msg.kind, msg.body)
        else if (msg.type === 'exit') {
            if (msg.locked) say('Open in another window', msg.message)
            else say('The editor stopped', msg.message, true)
            vscode.postMessage({type: 'error', message: msg.message})
        } else if (msg.type === 'look') {
            // The camera to a point (cells; with `world`, the game's units),
            // from a distance, at angles.
            const offset = msg.world ? terrain?.terrain?.offset ?? [0, 0] : [0, 0]
            const k = msg.world ? 128 : 1
            const [x, y] = [(msg.x - offset[0]) / k, (msg.y - offset[1]) / k]
            camera.position = [x, y, msg.z !== undefined ? msg.z / k : camera.heightAt(x, y)]
            if (msg.distance) camera.distance = msg.distance
            if (msg.horizontal !== undefined) camera.horizontal = msg.horizontal
            if (msg.vertical !== undefined) camera.vertical = msg.vertical
        } else if (msg.type === 'brush') {
            // A stroke at points of the view (-1 to 1, y up), as the mouse
            // would lay it, or at points of the map (`cells`, from above): for the tests.
            Object.assign(brush, msg.brush)
            showTools()
            const r = canvas.getBoundingClientRect()
            const rays = msg.cells
                ? msg.cells.map(([x, y]) => ({origin: [x, y, 1000], dir: [0, 0, -1]}))
                : msg.points.map(p => camera.ray(p[0], p[1], r.width / r.height))
            rays.forEach(({origin, dir}, i) => send(encode.brush(i === 0, origin, dir, brush)))
            send(encode.strokeEnd())
        } else if (msg.type === 'place') {
            // A type placed at a point of the view: for the tests.
            const r = canvas.getBoundingClientRect()
            const {origin, dir} = camera.ray(msg.x, msg.y, r.width / r.height)
            send(encode.place(msg.kind, msg.id, msg.player ?? 0, origin, dir))
        } else if (msg.type === 'drag') {
            // The thing picked dragged to a point of the view: for the tests.
            if (!selected) return
            const r = canvas.getBoundingClientRect()
            const {origin, dir} = camera.ray(msg.x, msg.y, r.width / r.height)
            send(encode.move(true, selected.file, selected.index, origin, dir))
            send(encode.strokeEnd())
        } else if (msg.type === 'region') {
            // A region's drag between points of the view: for the tests.
            const r = canvas.getBoundingClientRect()
            const ray = p => camera.ray(p[0], p[1], r.width / r.height)
            const index = msg.mode ? selectedRegion ?? -1 : -1
            send(encode.region(true, msg.mode, index, ray(msg.from), ray(msg.to)))
            send(encode.strokeEnd())
            if (msg.mode === 0) selectNewRegion = true
        } else if (msg.type === 'fov') {
            // The view's field of view (and roll): for the tests.
            camera.fov = msg.fov
            if (msg.roll !== undefined) camera.roll = msg.roll
        } else if (msg.type === 'camera') {
            // The Cameras tool's, for the tests: `new` from the view, `pick`
            // one (`index`), `drag` the one picked to a point of the view,
            // `set` it to the view, `look` through it, `rename`, a `field` typed, `delete`.
            const c = selectedCamera
            if (msg.action === 'new') $('camera-new').onclick()
            else if (msg.action === 'pick') pickCamera(msg.index)
            else if (msg.action === 'set') $('camera-set').onclick()
            else if (msg.action === 'look') {
                $('camera-look').onclick()
                vscode.postMessage({type: 'camera', camera: c !== null && {index: c, name: cameras[c].name, target: cameras[c].target, view: cameras[c].view, count: cameras.length, looking: {position: camera.position, distance: camera.distance, horizontal: camera.horizontal, vertical: camera.vertical, fov: camera.fov, roll: camera.roll}}})
            } else if (msg.action === 'rename' && c !== null) send(encode.cameraRename(c, msg.name))
            else if (msg.action === 'field') {
                // As typed in its field (`field_of_view` ...).
                const f = CAMERA_FIELDS.find(f => f.field === msg.field)
                f.input.value = String(msg.value)
                f.input.oninput()
            } else if (msg.action === 'delete') $('camera-delete').onclick()
            else if (msg.action === 'drag' && c !== null) {
                send(encode.camera(true, c, camera.ray(msg.x, msg.y, aspect())))
                send(encode.strokeEnd())
            }
        } else if (msg.type === 'usedBy') {
            showUsedBy(msg)
        } else if (msg.type === 'trg') {
            // The trigger editor's logic, for its panel.
            triggerEditor?.receive(msg.msg)
        } else if (msg.type === 'openUse') {
            // A use of what's picked clicked (`index`): for the tests.
            $('used-by').querySelectorAll('.used-row')[msg.index]?.onclick()
        } else if (msg.type === 'showOnMap') {
            // A name shown on the map, as a trigger's parameter dialog shows it: for the tests.
            send(encode.mapFind(msg.name))
        } else if (msg.type === 'pickOnMap') {
            // A pick on the map begun, as a trigger's parameter dialog begins it: for the tests.
            toggle(triggersPanel)
            startMapPick(msg.kind, () => {})
        } else if (msg.type === 'mapPickEsc') {
            if (mapPick) endMapPick(null)
        } else if (msg.type === 'mapPickAt') {
            // A click while picking on the map, at a point of the view (-1 to 1, y up): for the tests.
            const r = canvas.getBoundingClientRect()
            if (mapPick) mapPickAt(r.left + (msg.x + 1) / 2 * r.width, r.top + (1 - msg.y) / 2 * r.height)
        } else if (msg.type === 'button') {
            // A button of the page's pressed (`id`): for the tests.
            $(msg.id)?.onclick()
        } else if (msg.type === 'mapInfo') {
            // The map info panel's, for the tests: `open`, a field `set` (`field`, `value`, a flag's `bit`), `close`.
            if (msg.action === 'open') toggle(infoPanel)
            else if (msg.action === 'set') infoPanel.type(msg.field, msg.value, msg.bit)
            else if (msg.action === 'close') infoPanel.close()
        } else if (msg.type === 'import') {
            // The import manager's, for the tests: `open`, `add` (a file on
            // disk, at `path`), `pick` (`index`), `move` (to `path`), `delete`, `close`.
            if (msg.action === 'open') importsPanel.open()
            else if (msg.action === 'add') send(encode.importAdd(msg.source, msg.path ?? ''))
            else if (msg.action === 'pick') importsPanel.pick(msg.index)
            else if (msg.action === 'move') send(encode.importSet(importsPanel.picked, msg.path))
            else if (msg.action === 'delete') importsPanel.deleteButton.onclick()
            else if (msg.action === 'close') importsPanel.close()
        } else if (msg.type === 'object') {
            // The object editor's, for the tests: `open`, `kind` (`k`),
            // `pick` (`id`), a `field` typed (`field`, `level`, `value`),
            // `new` (based on the one picked), `delete`, `close`.
            if (msg.action === 'open') objectsPanel.open()
            else if (msg.action === 'kind') objectsPanel.showKind(msg.k)
            else if (msg.action === 'pick') objectsPanel.pick(msg.id)
            else if (msg.action === 'field') objectsPanel.type(msg.field, msg.level, msg.value)
            else if (msg.action === 'new') objectsPanel.newButton.onclick()
            else if (msg.action === 'copy') objectsPanel.copyButton.onclick()
            else if (msg.action === 'paste') objectsPanel.pasteButton.onclick()
            else if (msg.action === 'pickId') objectsPanel.pickId(msg.field, msg.level, msg.id)
            else if (msg.action === 'delete') objectsPanel.deleteButton.onclick()
            else if (msg.action === 'close') objectsPanel.close()
        } else if (msg.type === 'sound') {
            // The Sounds tool's, for the tests: `new` of a `file`, `pick`
            // one (`index`), a `field` typed (a flag's `bit` checked or not),
            // `play`, `delete`.
            if (msg.action === 'new') {
                $('sound-new-file').value = msg.file
                $('sound-new').onclick()
            } else if (msg.action === 'pick') {
                $('sound-list').value = String(msg.index)
                $('sound-list').onchange()
            } else if (msg.action === 'field') {
                const f = SOUND_FIELDS.find(f => f.field === msg.field && (f.bit ?? 0) === (msg.bit ?? 0))
                if (f.bit) {
                    f.input.checked = !!msg.value
                    f.input.onchange()
                } else if (f.options) {
                    f.input.value = String(msg.value)
                    f.input.onchange()
                } else {
                    f.input.value = String(msg.value)
                    f.input.oninput()
                }
            } else if (msg.action === 'browse') {
                // The browser opened, searched, the first found taken.
                if ($('sound-browser').hidden) $('sound-browse').onclick()
                $('sound-browse-search').value = msg.search ?? ''
                showSoundFiles()
            } else if (msg.action === 'takeFirst') {
                $('sound-browse-list').querySelector('[data-file]')?.onclick()
            } else if (msg.action === 'play') $('sound-play').onclick()
            else if (msg.action === 'delete') $('sound-delete').onclick()
        } else if (msg.type === 'select') {
            // A thing picked by its file and index: for the tests.
            selected = {file: msg.file, index: msg.index}
            const found = objects?.find(msg.file, msg.index)
            objects?.select(found ? found.model : -1, found ? found.instance : -1)
            askThing()
        } else if (msg.type === 'thingList') {
            // A list's input typed (`list`: `items:2`, `abilities:add`, `drops:new` ...): for the tests.
            const input = $('thing-lists').querySelector(`[data-list="${CSS.escape(msg.list)}"]`)
            if (input) {
                input.value = String(msg.value)
                ;(input.oninput ?? input.onchange)()
            }
        } else if (msg.type === 'thingField') {
            // A field of the thing picked typed (`scale_x` ...): for the tests.
            const f = THING_FIELDS.find(f => f.field === msg.field)
            f.input.value = String(msg.value)
            f.input.oninput()
        } else if (msg.type === 'tool') {
            // A tool picked, as its button does: for the tests.
            brush.tool = msg.tool
            showTools()
        } else if (msg.type === 'regionRename' && selectedRegion !== null) {
            send(encode.regionRename(selectedRegion, msg.name))
        } else if (msg.type === 'rotate' && selected) {
            send(encode.rotate(selected.file, selected.index, msg.delta))
        } else if (msg.type === 'delete' && selected) {
            send(encode.delete(selected.file, selected.index))
            selected = null
        } else if (msg.type === 'save') {
            save()
        } else if (msg.type === 'undo' || msg.type === 'redo') {
            send(encode[msg.type]())
        } else if (msg.type === 'unfocus' && focusedFrom) {
            Object.assign(camera, focusedFrom)
            focusedFrom = null
        } else if (msg.type === 'focusEffect' && objects) {
            // The camera close over the first thing whose model's path
            // has `path`, or has tails (`tails`), or ribbons (or else
            // tails): for the tests' pictures.
            const p = objects.particles
            const has = mi => (p.ribbons[mi]?.length ?? 0) > 0
            const tails = mi => (p.models[mi] ?? []).some(x => x.e.params[14] !== 0)
            const data = objects.instanceData ?? []
            const named = mi => !!msg.path && (objects.models[mi]?.path ?? '').toLowerCase().includes(msg.path.toLowerCase())
            const order = msg.path ? [named] : msg.tails ? [tails] : [has, tails]
            const mi = order.map(f => data.findIndex((d, i) => d.length && f(i))).find(i => i >= 0)
            if (mi !== undefined) {
                focusedFrom = {position: [...camera.position], distance: camera.distance, vertical: camera.vertical, horizontal: camera.horizontal}
                const d = data[mi]
                camera.position = [d[0], d[1], d[2] + 1]
                camera.distance = msg.distance ?? 8
                camera.vertical = -0.6
            }
            vscode.postMessage({type: 'focused', found: mi !== undefined, path: mi !== undefined ? objects.models[mi]?.path : ''})
        } else if (msg.type === 'flicker') {
            if (typeof msg.animate === 'boolean') {
                show.animate = msg.animate
                applyShow()
            }
            flicker = {want: msg.frames ?? 120, frames: [], watch: msg.watch, crops: []}
        } else if (msg.type === 'mapOpen') {
            mapPanel.open()
        } else if (msg.type === 'mapResize') {
            // As the panel's buttons do: for the tests.
            mapPanel.resize(msg.sides, msg.margins ?? null)
        } else if (msg.type === 'mapTiles') {
            send(encode.tiles(msg.ids, msg.to, msg.tileset))
        } else if (msg.type === 'tilePathing') {
            send(encode.tilePathing(msg.index, msg.forbids))
        } else if (msg.type === 'hours') {
            light.hours = msg.hours
            $('hours').value = String(msg.hours)
        } else if (msg.type === 'show') {
            // Layers turned on or off, as the buttons do: for the tests.
            Object.assign(show, msg.show)
            applyShow()
        } else if (msg.type === 'click') {
            // A click at a point of the view (-1 to 1, y up), as the mouse
            // would: for the tests.
            const r = canvas.getBoundingClientRect()
            const {origin, dir} = camera.ray(msg.x, msg.y, r.width / r.height)
            send(encode.pick(origin, dir))
        } else if (msg.type === 'restarted') {
            $('message').hidden = true
            send(encode.hello())
        }
    })
    $('reopen').onclick = () => {
        say('Opening…', '')
        vscode.postMessage({type: 'reopen'})
    }

    if (!gl) say('No WebGL 2', 'This VS Code can\'t draw the map: WebGL 2 isn\'t available.')
    else if (!terrain) {
        say('Can\'t draw', `The terrain's shaders don't compile here:\n${shaderError}`)
        vscode.postMessage({type: 'error', message: shaderError})
    }
    send(encode.hello())
})()
