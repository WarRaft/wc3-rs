/* global acquireVsCodeApi, EditorProtocol, EditorCamera, EditorTerrain, EditorObjects, EditorOverlays */

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
            terrain.draw(mvp, now / 1000)
            objects?.draw(mvp)
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
    // A click: what's under it, asked of the editor (it answers Picked).
    canvas.addEventListener('click', e => {
        if (!terrain?.ready) return
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
        if (e.target instanceof HTMLInputElement) return
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
    ]
    const show = {pathing: false, regions: false, cameras: false, bounds: true, minimap: true, ...(vscode.getState()?.show ?? {})}
    const toggles = $('toggles')
    function applyShow() {
        if (terrain) terrain.show = {pathing: show.pathing, regions: show.regions}
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

    function showNotes() {
        const all = [...mapNotes, ...(terrain?.notes ?? []), ...(objects?.notes ?? []), ...overlayNotes]
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
            if (!placed) {
                camera.reset(msg.width - 1, msg.height - 1)
                placed = true
            }
            showNotes()
        } else if (msg.kind === 'objects' && objects) {
            objects.notes = []
            objects.set(msg)
            showNotes()
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
                        error: gl.getError(),
                    },
                }))
            }
        } else if (msg.kind === 'overlays' && terrain) {
            terrain.setOverlays(msg)
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
        } else if (msg.kind === 'picked' && objects) {
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
