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
            const {x: right, forward} = camera.axes()
            const up = [right[1] * camera.direction[2] - right[2] * camera.direction[1], right[2] * camera.direction[0] - right[0] * camera.direction[2], right[0] * camera.direction[1] - right[1] * camera.direction[0]]
            objects?.draw(mvp, now / 1000, {right, up, forward, view: camera.direction, eye: camera.eye()}, light, hours)
            lines?.draw(mvp, show)
        }
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
        const mod = e.ctrlKey || e.metaKey
        if (mod && e.code === 'KeyZ') {
            e.preventDefault()
            send(e.shiftKey ? encode.redo() : encode.undo())
            return
        }
        if (mod && (e.code === 'KeyY' || e.code === 'KeyS')) {
            e.preventDefault()
            send(e.code === 'KeyS' ? encode.save() : encode.redo())
            return
        }
        if (mod || e.target instanceof HTMLInputElement || e.target instanceof HTMLSelectElement) return
        if (brush.tool === 'regions' && selectedRegion !== null && (e.code === 'Delete' || e.code === 'Backspace')) {
            $('region-delete').onclick()
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
        {key: 'regions', label: 'Regions', code: 'KeyR'},
        {key: 'cameras', label: 'Cameras', code: 'KeyC'},
        {key: 'bounds', label: 'Bounds', code: 'KeyB'},
        {key: 'minimap', label: 'Minimap', code: 'KeyM'},
        {key: 'animate', label: 'Animate', code: 'KeyN'},
        {key: 'shadows', label: 'Shadows', code: 'KeyH'},
        {key: 'fog', label: 'Fog', code: 'KeyF'},
        {key: 'cycle', label: 'Day cycle', code: 'KeyT'},
    ]
    const show = {pathing: false, regions: false, cameras: false, bounds: true, minimap: true, animate: true, shadows: true, fog: false, cycle: false, ...(vscode.getState()?.show ?? {})}
    const toggles = $('toggles')
    function applyShow() {
        if (terrain) terrain.show = {pathing: show.pathing, regions: show.regions}
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
        b.title = `${t.label} (${t.code.slice(3)})`
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
        {tool: 'water', label: 'Water'},
        {tool: 'dry', label: 'Dry'},
        {tool: 'place', label: 'Place'},
        {tool: 'regions', label: 'Regions'},
        {tool: 'pathingOn', label: 'Pathing +'},
        {tool: 'pathingOff', label: 'Pathing −'},
    ]
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
        $('size-label').hidden = ['place', 'select', 'regions'].includes(brush.tool)
        $('strength-label').hidden = ['place', 'select', 'regions'].includes(brush.tool) || pathing
        for (const c of $('pathing-flags').querySelectorAll('input')) c.checked = (brush.flags & Number(c.value)) !== 0
        // A tool of a layer shows the layer.
        if ((pathing && !show.pathing) || (brush.tool === 'regions' && !show.regions)) {
            if (pathing) show.pathing = true
            else show.regions = true
            applyShow()
        }
        for (const b of $('textures').children) b.classList.toggle('on', b.dataset.id === brush.texture)
        $('size-value').textContent = brush.size
        $('strength-value').textContent = brush.strength.toFixed(2)
        canvas.style.cursor = brush.tool === 'select' ? '' : 'crosshair'
        if (['select', 'place', 'regions'].includes(brush.tool) && terrain) terrain.brush = [0, 0, 0]
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
    $('save').onclick = () => send(encode.save())
    const mapPanel = new EditorMapSettings.Panel($('map-panel'), send, encode)
    $('map-open').onclick = () => (mapPanel.isOpen ? mapPanel.close() : mapPanel.open())
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
        if (e.button !== 0 || ['select', 'place', 'regions'].includes(brush.tool) || !terrain?.ready) return
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
        if (!['select', 'place', 'regions'].includes(brush.tool) && terrain?.ready) {
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
            const found = selected && objects.find(selected.file, selected.index)
            if (selected && !found) selected = null
            objects.select(found ? found.model : -1, found ? found.instance : -1)
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
            if (msg.minimap.width) {
                minimap.width = msg.minimap.width
                minimap.height = msg.minimap.height
                minimapImage = new ImageData(new Uint8ClampedArray(msg.minimap.data.buffer, msg.minimap.data.byteOffset, msg.minimap.data.byteLength), msg.minimap.width, msg.minimap.height)
            }
            overlayNotes = msg.notes
            showNotes()
            vscode.postMessage({type: 'overlays', overlays: {
                pathing: msg.pathing.width,
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
            showState(msg)
        } else if (msg.kind === 'light' && terrain) {
            light.set(msg)
            terrain.setShadows(msg.shadows)
            lightNotes = msg.notes
            showNotes()
            vscode.postMessage({type: 'light', light: {day: msg.terrain.length > 0, shadows: msg.shadows.width, fog: !!msg.fog}})
        } else if (msg.kind === 'icons') {
            for (const i of msg.icons) {
                const key = `${i.kind}:${i.id}`
                icons.cache.set(key, i.width ? new ImageData(new Uint8ClampedArray(i.rgba.buffer, i.rgba.byteOffset, i.rgba.byteLength), i.width, i.height) : null)
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
            $('status').textContent = msg.found ? msg.text : ''
            vscode.postMessage({type: 'picked', text: msg.found ? msg.text : ''})
        } else if (msg.kind === 'error') {
            $('name').textContent = 'Can\'t open the map'
            say('Can\'t open the map', msg.message, true)
            vscode.postMessage({type: 'error', message: msg.message})
        }
    }

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
            // would lay it: for the tests.
            Object.assign(brush, msg.brush)
            showTools()
            const r = canvas.getBoundingClientRect()
            msg.points.forEach((p, i) => {
                const {origin, dir} = camera.ray(p[0], p[1], r.width / r.height)
                send(encode.brush(i === 0, origin, dir, brush))
            })
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
        } else if (msg.type === 'undo' || msg.type === 'redo' || msg.type === 'save') {
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
