/* global acquireVsCodeApi, EditorProtocol, EditorCamera, EditorTerrain */

/**
 * The World Editor's page: it draws what the editor's process
 * (`wc3 editor`) sends and sends back what the user does; it never works
 * the map out itself. Frames come from the extension as `{type: 'frame',
 * kind, body}` and go to it as `{type: 'frame', data}` (see
 * `world-editor.js`); `{type: 'map'}` and `{type: 'error'}` tell the
 * extension what the page shows (its title, the tests).
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
    let shaderError = ''
    try {
        terrain = gl && new EditorTerrain.Terrain(gl)
    } catch (e) {
        shaderError = e.message
    }
    const camera = new EditorCamera.Camera()
    camera.heightAt = (x, y) => terrain?.heightAt(x, y) ?? 0
    let placed = false
    let mapNotes = []
    let drawnSaid = false

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
        if (terrain?.ready) terrain.draw(camera.matrix(w / h), now / 1000)
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
        requestAnimationFrame(frame)
    }
    requestAnimationFrame(frame)

    let drag = null
    canvas.addEventListener('contextmenu', e => e.preventDefault())
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
        if (e.code === 'KeyG' && terrain) terrain.grid = !terrain.grid
        else if (e.code === 'Home' && terrain?.terrain) camera.reset(terrain.terrain.width - 1, terrain.terrain.height - 1)
        else held.add(e.code)
    })
    window.addEventListener('keyup', e => held.delete(e.code))
    window.addEventListener('blur', () => held.clear())

    function showNotes() {
        const all = [...mapNotes, ...(terrain?.notes ?? [])]
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
        } else if (msg.kind === 'terrain' && terrain) {
            terrain.setTerrain(msg)
            if (!placed) {
                camera.reset(msg.width - 1, msg.height - 1)
                placed = true
            }
            showNotes()
            if (!drawnSaid) {
                drawnSaid = true
                requestAnimationFrame(() => vscode.postMessage({
                    type: 'drawn',
                    terrain: {width: msg.width, height: msg.height, layers: terrain.layers ? 1 : 0, water: terrain.water?.count ?? 0, error: gl.getError()},
                }))
            }
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
