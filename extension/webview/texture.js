// @ts-check
'use strict'

/**
 * The texture viewer, a webview page (`extension.js`, `registerTextures`).
 * The extension posts the decoded texture: the header, and every mip as
 * RGBA. Everything here is drawing: which mip, which channels, on what
 * background, how big; the pixel under the cursor; saving a mip as PNG
 * (encoded here by the canvas, written by the extension).
 *
 * A pathing map (`.wpm`) comes as a texture too, with its cells' flags
 * (`cells`, one byte per pixel) and their names (`flags`): the page shows
 * its picture, or one flag at a time (white where it's set), and names a
 * cell's flags under the cursor.
 *
 * Messages in: `{type: 'texture', size, texture}`, `{type: 'error', message}`.
 * Messages out: `{type: 'ready'}`, `{type: 'save', mip, png}`.
 */

// @ts-ignore: provided by VS Code
const vscode = acquireVsCodeApi()

/** @typedef {{width: number, height: number, error: string, rgba: Uint8Array}} Mip */
/** @typedef {{error: string, version: string, encoding: string, alphaBits: number, width: number, height: number, mips: Mip[], cells: Uint8Array, flags: {bit: number, name: string}[]}} Texture */

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id))

const ZOOM_MIN = 1 / 32
const ZOOM_MAX = 64
const ZOOM_STEP = 1.25

/** What the page remembers across reloads of the same editor. */
const state = Object.assign({mip: 0, channels: 'rgba', background: 'checker', zoom: 'fit', flag: 0}, vscode.getState() || {})

/** Is the texture a pathing map: has it cells? */
function isPathing() {
    return !!texture && texture.cells && texture.cells.length > 0
}

/** @type {Texture | null} */
let texture = null

const canvas = /** @type {HTMLCanvasElement} */ ($('canvas'))
const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'))
const stage = $('stage')
const frame = $('frame')

/** Pixels per texel when the zoom is `fit`, else the zoom itself. */
function scale() {
    if (state.zoom !== 'fit') return state.zoom
    const mip = currentMip()
    if (!mip) return 1
    const style = getComputedStyle(stage)
    const pad = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight)
    const w = (stage.clientWidth - pad) / mip.width
    const h = (stage.clientHeight - pad) / mip.height
    const fit = Math.min(w, h)
    // Small textures grow by whole steps so texels stay square.
    return fit >= 1 ? Math.max(1, Math.floor(fit)) : fit
}

function currentMip() {
    return texture?.mips[state.mip] ?? null
}

function save() {
    vscode.setState(state)
}

/** The mip's pixels as the chosen channels show them; a pathing map's as
 * its picture, or one flag (`state.flag`): white where it's set. */
function pixels(/** @type {Mip} */ mip) {
    const src = mip.rgba
    if (isPathing() && state.flag) {
        const cells = /** @type {Texture} */ (texture).cells
        const out = new Uint8ClampedArray(src.length)
        for (let i = 0; i < cells.length; i++) {
            const on = cells[i] & state.flag ? 255 : 0
            out[i * 4] = out[i * 4 + 1] = out[i * 4 + 2] = on
            out[i * 4 + 3] = 255
        }
        return new ImageData(out, mip.width, mip.height)
    }
    if (state.channels === 'rgba') return new ImageData(new Uint8ClampedArray(src.buffer, src.byteOffset, src.byteLength), mip.width, mip.height)
    const out = new Uint8ClampedArray(src.length)
    for (let i = 0; i < src.length; i += 4) {
        if (state.channels === 'rgb') {
            out[i] = src[i]
            out[i + 1] = src[i + 1]
            out[i + 2] = src[i + 2]
        } else {
            out[i] = out[i + 1] = out[i + 2] = src[i + 3]
        }
        out[i + 3] = 255
    }
    return new ImageData(out, mip.width, mip.height)
}

function draw() {
    const mip = currentMip()
    const usable = mip && mip.rgba.length > 0
    frame.hidden = !usable
    $('missing').hidden = !!usable
    if (!mip) return
    if (!usable) {
        $('missing-text').textContent = `Mip ${state.mip} (${mip.width}×${mip.height}): ${mip.error}`
        return
    }
    canvas.width = mip.width
    canvas.height = mip.height
    ctx.putImageData(pixels(mip), 0, 0)
    layout()
}

/** Sizes the canvas for the zoom; the image stays as drawn. */
function layout() {
    const mip = currentMip()
    if (!mip) return
    const s = scale()
    canvas.style.width = `${mip.width * s}px`
    canvas.style.height = `${mip.height * s}px`
    // Shrunk images look better smoothed; grown ones show their texels.
    stage.classList.toggle('smooth', s < 1)
    $('zoom-value').textContent = `${Math.round(s * 100)}%`
    $('zoom-fit').setAttribute('aria-pressed', String(state.zoom === 'fit'))
    $('zoom-1').setAttribute('aria-pressed', String(state.zoom === 1))
}

/** @param {number} zoom  pixels per texel, or `fit` */
function setZoom(zoom, anchor = null) {
    const before = scale()
    const next = typeof zoom === 'number' ? Math.min(ZOOM_MAX, Math.max(ZOOM_MIN, zoom)) : zoom
    // Keep the point under the cursor where it is.
    const rect = canvas.getBoundingClientRect()
    const ax = anchor ? (anchor.x - rect.left) / before : 0
    const ay = anchor ? (anchor.y - rect.top) / before : 0
    state.zoom = next
    layout()
    if (anchor) {
        const after = scale()
        const moved = canvas.getBoundingClientRect()
        stage.scrollLeft += moved.left + ax * after - anchor.x
        stage.scrollTop += moved.top + ay * after - anchor.y
    }
    save()
}

function zoomBy(/** @type {number} */ factor, anchor = null) {
    setZoom(scale() * factor, anchor)
}

/** Builds a group of toggle buttons; `pick` runs on a click. */
function toggles(/** @type {HTMLElement} */ group, /** @type {[string, string, string?][]} */ items, /** @type {string} */ current, /** @type {(v: string) => void} */ pick) {
    group.querySelectorAll('button').forEach(b => b.remove())
    for (const [value, text, title] of items) {
        const b = document.createElement('button')
        b.textContent = text
        if (title) b.title = title
        b.setAttribute('aria-pressed', String(value === current))
        b.onclick = () => {
            group.querySelectorAll('button').forEach(o => o.setAttribute('aria-pressed', String(o === b)))
            pick(value)
        }
        group.append(b)
    }
}

function bytes(/** @type {number} */ n) {
    if (n < 1024) return `${n} B`
    if (n < 1048576) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / 1048576).toFixed(1)} MB`
}

function show(/** @type {Texture} */ t, /** @type {number} */ size) {
    texture = t
    $('loading').hidden = true
    $('error').hidden = true
    $('viewer').hidden = false
    if (state.mip >= t.mips.length) state.mip = 0

    // A texture without alpha has nothing for the A view to show.
    if (t.alphaBits === 0 && state.channels !== 'rgba') state.channels = 'rgba'
    $('channels').hidden = t.alphaBits === 0 && !isPathing()

    const kind = !isPathing()
        ? (t.alphaBits ? `alpha ${t.alphaBits} bit` : 'no alpha')
        : t.encoding === 'Shadows' ? 'dark: shadow' : 'red no walking, green no flying, blue no building'
    $('info').textContent = [t.version, t.encoding, `${t.width}×${t.height}`, kind, bytes(size)].join(' · ')
    $('info').title = `${t.mips.length} mip${t.mips.length === 1 ? '' : 's'}`

    const mips = $('mips')
    mips.querySelectorAll('button').forEach(b => b.remove())
    t.mips.forEach((m, i) => {
        const b = document.createElement('button')
        b.textContent = `${m.width}×${m.height}`
        b.title = m.error ? `Mip ${i}: ${m.error}` : `Mip ${i}`
        if (m.error) b.classList.add('missing')
        b.setAttribute('aria-pressed', String(i === state.mip))
        b.onclick = () => selectMip(i)
        mips.append(b)
    })
    mips.hidden = t.mips.length < 2

    if (isPathing()) {
        // The picture, or one flag at a time.
        $('channels-label').textContent = 'Show'
        if (!t.flags.some(f => f.bit === state.flag)) state.flag = 0
        const items = /** @type {[string, string, string?][]} */ ([['0', t.encoding, t.encoding === 'Shadows' ? 'Dark: shadow' : 'Red: no walking, green: no flying, blue: no building']])
        for (const f of t.flags) items.push([String(f.bit), `0x${f.bit.toString(16).padStart(2, '0')}`, f.name])
        toggles($('channels'), items, String(state.flag), v => {
            state.flag = Number(v)
            save()
            draw()
        })
    } else {
        $('channels-label').textContent = 'Channels'
        toggles($('channels'), [['rgba', 'RGBA'], ['rgb', 'RGB', 'Ignore alpha'], ['a', 'A', 'Alpha as grayscale']], state.channels, v => {
            state.channels = v
            save()
            draw()
        })
    }
    toggles($('background'), [['checker', '▦', 'Checkerboard'], ['black', '■', 'Black'], ['white', '□', 'White']], state.background, v => {
        state.background = v
        frame.className = `frame ${v}`
        save()
    })
    frame.className = `frame ${state.background}`
    draw()
}

function selectMip(/** @type {number} */ i) {
    if (!texture || i < 0 || i >= texture.mips.length) return
    state.mip = i
    $('mips').querySelectorAll('button').forEach((b, j) => b.setAttribute('aria-pressed', String(j === i)))
    save()
    draw()
}

function fail(/** @type {string} */ message) {
    texture = null
    $('loading').hidden = true
    $('viewer').hidden = true
    $('error').hidden = false
    $('error-text').textContent = message
}

// The pixel under the cursor.
canvas.addEventListener('mousemove', e => {
    const mip = currentMip()
    if (!mip || !mip.rgba.length) return
    const s = scale()
    const rect = canvas.getBoundingClientRect()
    const x = Math.min(mip.width - 1, Math.max(0, Math.floor((e.clientX - rect.left) / s)))
    const y = Math.min(mip.height - 1, Math.max(0, Math.floor((e.clientY - rect.top) / s)))
    const i = (y * mip.width + x) * 4
    const [r, g, b, a] = mip.rgba.subarray(i, i + 4)
    const hex = [r, g, b, a].map(v => v.toString(16).padStart(2, '0')).join('').toUpperCase()
    if (isPathing()) {
        // The cell, counted from the bottom left as the file does, and its
        // flags by name.
        const t = /** @type {Texture} */ (texture)
        const cell = t.cells[y * mip.width + x]
        const names = t.flags.filter(f => cell & f.bit).map(f => f.name)
        $('pos').textContent = `${x}, ${mip.height - 1 - y}`
        $('rgba').textContent = names.length ? names.join(', ') : 'no flags'
        $('hex').textContent = `0x${cell.toString(16).padStart(2, '0').toUpperCase()}`
    } else {
        $('pos').textContent = `${x}, ${y}`
        $('rgba').textContent = `R ${r}  G ${g}  B ${b}  A ${a}`
        $('hex').textContent = `#${hex}`
    }
    $('swatch').style.background = `rgba(${r}, ${g}, ${b}, ${a / 255})`
    $('pixel').hidden = false
})
canvas.addEventListener('mouseleave', () => {
    $('pixel').hidden = true
})

// Ctrl/Cmd + wheel (and a trackpad pinch) zooms around the cursor.
stage.addEventListener('wheel', e => {
    if (!e.ctrlKey && !e.metaKey) return
    e.preventDefault()
    zoomBy(Math.exp(-e.deltaY * 0.01), {x: e.clientX, y: e.clientY})
}, {passive: false})

$('zoom-in').onclick = () => zoomBy(ZOOM_STEP)
$('zoom-out').onclick = () => zoomBy(1 / ZOOM_STEP)
$('zoom-fit').onclick = () => setZoom('fit')
$('zoom-1').onclick = () => setZoom(1)

$('save').onclick = () => {
    const mip = currentMip()
    if (!mip || !mip.rgba.length) return
    // The mip itself, whatever the view shows.
    const out = document.createElement('canvas')
    out.width = mip.width
    out.height = mip.height
    const c = /** @type {CanvasRenderingContext2D} */ (out.getContext('2d'))
    c.putImageData(new ImageData(new Uint8ClampedArray(mip.rgba), mip.width, mip.height), 0, 0)
    out.toBlob(async blob => {
        if (!blob) return
        vscode.postMessage({type: 'save', mip: state.mip, png: new Uint8Array(await blob.arrayBuffer())})
    }, 'image/png')
}

document.addEventListener('keydown', e => {
    if (e.target instanceof HTMLInputElement) return
    if (e.key === '+' || e.key === '=') zoomBy(ZOOM_STEP)
    else if (e.key === '-') zoomBy(1 / ZOOM_STEP)
    else if (e.key === '0') setZoom('fit')
    else if (e.key === '1') setZoom(1)
    else if (e.key === '[' || e.key === 'PageUp') selectMip(state.mip - 1)
    else if (e.key === ']' || e.key === 'PageDown') selectMip(state.mip + 1)
    else return
    e.preventDefault()
})

new ResizeObserver(() => state.zoom === 'fit' && layout()).observe(stage)

window.addEventListener('message', e => {
    const msg = e.data
    if (msg.type === 'texture') {
        if (msg.texture.error) fail(msg.texture.error)
        else show(msg.texture, msg.size)
    } else if (msg.type === 'error') {
        fail(msg.message)
    }
})

vscode.postMessage({type: 'ready'})
