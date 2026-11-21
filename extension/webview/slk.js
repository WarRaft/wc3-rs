// @ts-check
'use strict'

/**
 * The SLK table editor, a webview page (`extension.js`, `registerSlk`).
 * The text stays the document: the extension posts the sheet the server
 * reads from it, and this page asks for a cell to be written; the
 * extension has the server turn that into text edits and applies them,
 * then posts the new sheet.
 *
 * The grid is drawn on a canvas, only what's in view: a sheet has tens of
 * thousands of cells, too many DOM nodes for a webview. A transparent
 * scroller over the canvas, holding an empty block the sheet's size, gives
 * native scrolling and scrollbars; the canvas redraws on every scroll. The
 * column letters, row 1 (the field names, when it's the header), the row
 * numbers and column A stay in place. The only other nodes are the cell
 * editor and a tooltip.
 *
 * Messages in: `{type: 'table', table}`, where `table` is
 * `{rows, cols, errors, cells: [[row, col, kind, text, comment, formula]]}`
 * (`sheet::table` in the server).
 * Messages out: `{type: 'ready'}`, `{type: 'set', row, col, text}`,
 * `{type: 'openText'}`.
 */

// @ts-ignore: provided by VS Code
const vscode = acquireVsCodeApi()

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id))

/** Row height in CSS pixels. */
const ROW_H = 22
/** Text padding inside a cell. */
const PAD = 6
/** Empty rows and columns past the sheet, to write new cells into. */
const EXTRA_ROWS = 5
const EXTRA_COLS = 3

/** @typedef {{kind: string, text: string, comment: string | null, formula: string | null}} Cell */

/** What the page remembers across reloads of the same editor. */
const state = Object.assign({header: true, freeze: true, filter: '', row: 1, col: 1}, vscode.getState() || {})
const save = () => vscode.setState(state)

/** @type {Map<string, Cell>} */
let cells = new Map()
let rows = 0
let cols = 0
let errors = 0
let loaded = false
/** Column widths in CSS pixels, by column (index 0 unused). */
let widths = /** @type {number[]} */ ([0])
/** Where each column starts, scrolled to the left (index 0 unused). */
let starts = /** @type {number[]} */ ([0])
/** The row numbers' width. */
let gutter = 48
/** The body's rows, after the header and the search. */
let order = /** @type {number[]} */ ([])
/** The cell being edited, if any. */
let editing = /** @type {{row: number, col: number} | null} */ (null)

const area = $('area')
const canvas = /** @type {HTMLCanvasElement} */ ($('canvas'))
const ctx = /** @type {CanvasRenderingContext2D} */ (canvas.getContext('2d'))
const scroll = $('scroll')
const sizer = $('sizer')
const editor = /** @type {HTMLInputElement} */ ($('editor'))
const tooltip = $('tooltip')
const search = /** @type {HTMLInputElement} */ ($('search'))
const headerBox = /** @type {HTMLInputElement} */ ($('header'))
const freezeBox = /** @type {HTMLInputElement} */ ($('freeze'))

const key = (/** @type {number} */ r, /** @type {number} */ c) => `${r},${c}`
const cell = (/** @type {number} */ r, /** @type {number} */ c) => cells.get(key(r, c))

/** `A`, `B`, ..., `Z`, `AA`, ... */
function letter(/** @type {number} */ c) {
    let s = ''
    for (; c > 0; c = Math.floor((c - 1) / 26)) s = String.fromCharCode(65 + ((c - 1) % 26)) + s
    return s
}

/** The rows and columns shown: the sheet and some room past it. */
const shownRows = () => Math.max(rows, 1) + EXTRA_ROWS
const shownCols = () => Math.max(cols, 1) + EXTRA_COLS
/** The height of the fixed rows on top: the letters, and row 1 as the header. */
const headHeight = () => ROW_H * (state.header ? 2 : 1)
/** Where the scrolling columns start on screen. */
const frozenRight = () => gutter + (state.freeze ? widths[1] || 0 : 0)

// Theme

/** Colors from the VS Code theme, read again when it changes. */
let theme = /** @type {Record<string, string>} */ ({})
let font = ''
let boldFont = ''
let italicFont = ''

function readTheme() {
    const style = getComputedStyle(document.documentElement)
    const v = (/** @type {string} */ name, /** @type {string} */ fallback) => style.getPropertyValue(name).trim() || fallback
    theme = {
        bg: v('--vscode-editor-background', '#1e1e1e'),
        fg: v('--vscode-editor-foreground', '#d4d4d4'),
        headBg: v('--vscode-editorWidget-background', '#252526'),
        headFg: v('--vscode-descriptionForeground', '#9d9d9d'),
        line: v('--vscode-editorGroup-border', 'rgba(128,128,128,0.35)'),
        number: v('--vscode-debugTokenExpression-number', '#b5cea8'),
        bool: v('--vscode-debugTokenExpression-boolean', '#569cd6'),
        error: v('--vscode-errorForeground', '#f48771'),
        warn: v('--vscode-editorWarning-foreground', '#cca700'),
        focus: v('--vscode-focusBorder', '#007fd4'),
        selection: v('--vscode-editor-selectionBackground', 'rgba(38,79,120,0.6)'),
        match: v('--vscode-editor-findMatchHighlightBackground', 'rgba(234,92,0,0.33)'),
    }
    const family = v('--vscode-editor-font-family', 'monospace')
    const size = v('--vscode-editor-font-size', '13px')
    font = `${size} ${family}`
    boldFont = `600 ${size} ${family}`
    italicFont = `italic ${size} ${family}`
}

// Layout

/** Column widths from the text in them: the header and a sample of rows. */
function measure() {
    ctx.font = font
    widths = [0]
    const sample = Math.min(shownRows(), 400)
    for (let c = 1; c <= shownCols(); c++) {
        let w = ctx.measureText(letter(c)).width
        for (let r = 1; r <= sample; r++) {
            const t = cell(r, c)
            if (t) w = Math.max(w, ctx.measureText(t.text).width)
        }
        widths.push(Math.round(Math.min(Math.max(w + 2 * PAD + 2, 48), 320)))
    }
    gutter = Math.round(ctx.measureText(String(shownRows())).width + 2 * PAD + 4)
    layout()
}

/** Column starts and the scroller's size, for the widths and the rows shown. */
function layout() {
    starts = [0]
    let x = gutter
    for (let c = 1; c <= shownCols(); c++) {
        starts.push(x)
        x += widths[c] || 80
    }
    sizer.style.width = `${x}px`
    sizer.style.height = `${headHeight() + order.length * ROW_H}px`
}

/** The body rows: all but row 1 when it's the header, those that match the search. */
function filter() {
    const needle = state.filter.toLowerCase()
    order = []
    for (let r = state.header ? 2 : 1; r <= shownRows(); r++) {
        if (needle && !rowMatches(r, needle)) continue
        order.push(r)
    }
    layout()
}

function rowMatches(/** @type {number} */ r, /** @type {string} */ needle) {
    for (let c = 1; c <= cols; c++) {
        const t = cell(r, c)
        if (t && t.text.toLowerCase().includes(needle)) return true
    }
    return false
}

/** Where column `c` is on screen. */
function colX(/** @type {number} */ c) {
    return c === 1 && state.freeze ? gutter : starts[c] - scroll.scrollLeft
}

/** Where row `r` is on screen, or null when it isn't shown. */
function rowY(/** @type {number} */ r) {
    if (state.header && r === 1) return ROW_H
    const i = order.indexOf(r)
    return i < 0 ? null : headHeight() + i * ROW_H - scroll.scrollTop
}

/** The cell at a point on screen. */
function hit(/** @type {number} */ x, /** @type {number} */ y) {
    if (x < gutter || y < ROW_H) return null
    let row
    if (state.header && y < headHeight()) row = 1
    else row = order[Math.floor((y - headHeight() + scroll.scrollTop) / ROW_H)]
    if (row === undefined) return null
    let col = 0
    if (state.freeze && x < gutter + widths[1]) col = 1
    else {
        const sx = x + scroll.scrollLeft
        for (let c = state.freeze ? 2 : 1; c <= shownCols(); c++) {
            if (sx >= starts[c] && sx < starts[c] + widths[c]) {
                col = c
                break
            }
        }
    }
    return col ? {row, col} : null
}

// Drawing

let frame = 0
/** Redraws on the next frame. */
function draw() {
    if (!frame) frame = requestAnimationFrame(paint)
}

function paint() {
    frame = 0
    const w = scroll.clientWidth
    const h = scroll.clientHeight
    const dpr = window.devicePixelRatio || 1
    if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
        canvas.width = Math.round(w * dpr)
        canvas.height = Math.round(h * dpr)
        canvas.style.width = `${w}px`
        canvas.style.height = `${h}px`
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.fillStyle = theme.bg
    ctx.fillRect(0, 0, w, h)
    if (!loaded) return
    ctx.textBaseline = 'middle'

    const head = headHeight()
    const first = Math.max(0, Math.floor(scroll.scrollTop / ROW_H))
    const last = Math.min(order.length, first + Math.ceil((h - head) / ROW_H) + 1)
    const left = frozenRight()
    // The columns in view, past the frozen ones.
    const visible = []
    for (let c = state.freeze ? 2 : 1; c <= shownCols(); c++) {
        const x = colX(c)
        if (x + widths[c] < left) continue
        if (x > w) break
        visible.push(c)
    }

    // The body, scrolling both ways.
    clip(left, head, w - left, h - head, () => {
        for (let i = first; i < last; i++) {
            const y = head + i * ROW_H - scroll.scrollTop
            for (const c of visible) drawCell(order[i], c, colX(c), y, false)
        }
    })
    // Column A, scrolling down only.
    if (state.freeze) {
        clip(gutter, head, widths[1], h - head, () => {
            for (let i = first; i < last; i++) drawCell(order[i], 1, gutter, head + i * ROW_H - scroll.scrollTop, false)
        })
    }
    // Row 1, the header, scrolling across only.
    if (state.header) {
        clip(left, ROW_H, w - left, ROW_H, () => {
            for (const c of visible) drawCell(1, c, colX(c), ROW_H, true)
        })
        if (state.freeze) drawCell(1, 1, gutter, ROW_H, true)
    }
    // The column letters.
    ctx.font = font
    clip(left, 0, w - left, ROW_H, () => {
        for (const c of visible) drawHead(letter(c), colX(c), 0, widths[c], 'center')
    })
    if (state.freeze) drawHead('A', gutter, 0, widths[1], 'center')
    // The row numbers.
    clip(0, head, gutter, h - head, () => {
        for (let i = first; i < last; i++) drawHead(String(order[i]), 0, head + i * ROW_H - scroll.scrollTop, gutter, 'right')
    })
    if (state.header) drawHead('1', 0, ROW_H, gutter, 'right')
    drawHead('', 0, 0, gutter, 'right')
    // Lines under the fixed parts.
    ctx.fillStyle = theme.line
    ctx.fillRect(0, head - 1, w, 1)
    ctx.fillRect(gutter - 1, 0, 1, h)
    if (state.freeze) ctx.fillRect(left - 2, 0, 2, h)
    drawSelection()
}

/** Runs `body` with drawing kept to a rectangle. */
function clip(/** @type {number} */ x, /** @type {number} */ y, /** @type {number} */ w, /** @type {number} */ h, /** @type {() => void} */ body) {
    if (w <= 0 || h <= 0) return
    ctx.save()
    ctx.beginPath()
    ctx.rect(x, y, w, h)
    ctx.clip()
    body()
    ctx.restore()
}

/** A header cell: a column letter or a row number. */
function drawHead(/** @type {string} */ text, /** @type {number} */ x, /** @type {number} */ y, /** @type {number} */ w, /** @type {'center' | 'right'} */ align) {
    ctx.fillStyle = theme.headBg
    ctx.fillRect(x, y, w, ROW_H)
    ctx.fillStyle = theme.line
    ctx.fillRect(x + w - 1, y, 1, ROW_H)
    ctx.fillRect(x, y + ROW_H - 1, w, 1)
    if (!text) return
    ctx.font = font
    ctx.fillStyle = theme.headFg
    ctx.textAlign = align
    ctx.fillText(text, align === 'center' ? x + w / 2 : x + w - PAD, y + ROW_H / 2)
}

/** One cell: its background, text, comment mark and grid lines. */
function drawCell(/** @type {number} */ r, /** @type {number} */ c, /** @type {number} */ x, /** @type {number} */ y, /** @type {boolean} */ isHeader) {
    const w = widths[c]
    const t = cell(r, c)
    ctx.fillStyle = isHeader ? theme.headBg : theme.bg
    ctx.fillRect(x, y, w, ROW_H)
    if (t && state.filter && t.text.toLowerCase().includes(state.filter.toLowerCase())) {
        ctx.fillStyle = theme.match
        ctx.fillRect(x, y, w, ROW_H)
    }
    if (r === state.row && c === state.col && document.activeElement === scroll) {
        ctx.fillStyle = theme.selection
        ctx.fillRect(x, y, w, ROW_H)
    }
    if (t && t.text && !(editing && editing.row === r && editing.col === c)) {
        ctx.font = isHeader ? boldFont : t.formula ? italicFont : font
        ctx.fillStyle = {n: theme.number, b: theme.bool, e: theme.error, x: theme.error}[t.kind] || theme.fg
        const right = t.kind === 'n'
        ctx.textAlign = right ? 'right' : 'left'
        ctx.save()
        ctx.beginPath()
        ctx.rect(x + 1, y, w - 2, ROW_H)
        ctx.clip()
        ctx.fillText(t.text.replace(/[\r\n\t]+/g, ' '), right ? x + w - PAD : x + PAD, y + ROW_H / 2 + 1)
        ctx.restore()
        if (t.kind === 'x') {
            // Not a value the format has: underline it like an error.
            ctx.fillStyle = theme.error
            ctx.fillRect(x + PAD, y + ROW_H - 4, Math.min(ctx.measureText(t.text).width, w - 2 * PAD), 1)
        }
    }
    if (t && t.comment) {
        ctx.fillStyle = theme.warn
        ctx.beginPath()
        ctx.moveTo(x + w - 7, y)
        ctx.lineTo(x + w - 1, y)
        ctx.lineTo(x + w - 1, y + 6)
        ctx.closePath()
        ctx.fill()
    }
    ctx.fillStyle = theme.line
    ctx.fillRect(x + w - 1, y, 1, ROW_H)
    ctx.fillRect(x, y + ROW_H - 1, w, 1)
}

/** The frame around the selected cell, where it's in view. */
function drawSelection() {
    const y = rowY(state.row)
    if (y === null || !widths[state.col]) return
    const x = colX(state.col)
    const top = state.header && state.row === 1 ? ROW_H : headHeight()
    const left = state.col === 1 && state.freeze ? gutter : frozenRight()
    clip(left, top, scroll.clientWidth - left, scroll.clientHeight - top, () => {
        ctx.strokeStyle = theme.focus
        ctx.lineWidth = 2
        ctx.strokeRect(x + 1, y + 1, widths[state.col] - 2, ROW_H - 2)
    })
}

// Selection

function info() {
    const shown = state.filter ? `${order.length} rows shown · ` : ''
    $('info').textContent = `${shown}${rows} × ${cols}`
    const problems = $('problems')
    problems.hidden = errors === 0
    problems.textContent = errors === 1 ? '1 problem in the text' : `${errors} problems in the text`
}

/** Every row the selection moves through: the header's, then the body's. */
const allRows = () => (state.header ? [1, ...order] : order)

/** Moves the selection, keeping it in view. */
function select(/** @type {number} */ r, /** @type {number} */ c) {
    state.row = r
    state.col = Math.min(Math.max(c, 1), shownCols())
    save()
    reveal()
    draw()
}

/** Scrolls the selection into view. */
function reveal() {
    const i = order.indexOf(state.row)
    if (i >= 0) {
        const top = i * ROW_H
        const room = scroll.clientHeight - headHeight()
        if (top < scroll.scrollTop) scroll.scrollTop = top
        else if (top + ROW_H > scroll.scrollTop + room) scroll.scrollTop = top + ROW_H - room
    }
    if (state.col === 1 && state.freeze) return
    const left = starts[state.col]
    const right = left + widths[state.col]
    const fixed = frozenRight()
    if (left - fixed < scroll.scrollLeft) scroll.scrollLeft = left - fixed
    else if (right > scroll.scrollLeft + scroll.clientWidth) scroll.scrollLeft = right - scroll.clientWidth
}

/** Moves by `dr` rows (through the shown rows) and `dc` columns. */
function move(/** @type {number} */ dr, /** @type {number} */ dc) {
    const list = allRows()
    if (!list.length) return
    let i = list.indexOf(state.row)
    if (i < 0) i = 0
    i = Math.min(Math.max(i + dr, 0), list.length - 1)
    select(list[i], state.col + dc)
}

// Editing

function startEdit(/** @type {string | null} */ initial) {
    reveal()
    editing = {row: state.row, col: state.col}
    const t = cell(state.row, state.col)
    editor.value = initial ?? (t ? t.text : '')
    editor.hidden = false
    placeEditor()
    editor.focus()
    if (initial === null) editor.select()
    draw()
}

/** Puts the editor over its cell. */
function placeEditor() {
    if (!editing) return
    const y = rowY(editing.row)
    if (y === null) {
        editor.style.left = '-10000px'
        return
    }
    editor.style.left = `${colX(editing.col)}px`
    editor.style.top = `${y}px`
    editor.style.width = `${Math.max(widths[editing.col], 160)}px`
    editor.style.height = `${ROW_H}px`
}

/** Writes the cell (the extension edits the text) and shows it right away. */
function write(/** @type {number} */ r, /** @type {number} */ c, /** @type {string} */ text) {
    const old = cell(r, c)
    if ((old ? old.text : '') === text) return
    vscode.postMessage({type: 'set', row: r, col: c, text})
    if (text === '' && !(old && (old.comment || old.formula))) cells.delete(key(r, c))
    else cells.set(key(r, c), {kind: old ? old.kind : '', text, comment: old ? old.comment : null, formula: old ? old.formula : null})
}

function endEdit(/** @type {boolean} */ commit) {
    if (!editing) return
    const {row, col} = editing
    editing = null
    editor.hidden = true
    if (commit) write(row, col, editor.value)
    scroll.focus()
    draw()
}

editor.addEventListener('keydown', e => {
    if (e.key === 'Enter') {
        e.preventDefault()
        endEdit(true)
        move(e.shiftKey ? -1 : 1, 0)
    } else if (e.key === 'Tab') {
        e.preventDefault()
        endEdit(true)
        move(0, e.shiftKey ? -1 : 1)
    } else if (e.key === 'Escape') {
        e.preventDefault()
        endEdit(false)
    }
    e.stopPropagation()
})
editor.addEventListener('blur', () => endEdit(true))

// Mouse and keyboard

/** A mouse event's point over the canvas. */
function point(/** @type {MouseEvent} */ e) {
    const box = scroll.getBoundingClientRect()
    return [e.clientX - box.left, e.clientY - box.top]
}

scroll.addEventListener('mousedown', e => {
    const [x, y] = point(e)
    // Clicks on the scrollbars scroll.
    if (x >= scroll.clientWidth || y >= scroll.clientHeight) return
    const at = hit(x, y)
    scroll.focus()
    if (at) select(at.row, at.col)
    e.preventDefault()
})
scroll.addEventListener('dblclick', e => {
    const [x, y] = point(e)
    if (hit(x, y)) startEdit(null)
})

let tipAt = ''
scroll.addEventListener('mousemove', e => {
    const [x, y] = point(e)
    const at = hit(x, y)
    const t = at && cell(at.row, at.col)
    const tips = []
    if (t && t.formula) tips.push(`= ${t.formula}`)
    if (t && t.comment) tips.push(t.comment)
    if (t && t.text && ctx.measureText(t.text).width > widths[at.col] - 2 * PAD) tips.unshift(t.text)
    const id = at ? key(at.row, at.col) : ''
    if (!tips.length) {
        tooltip.hidden = true
        tipAt = ''
        return
    }
    if (id === tipAt) return
    tipAt = id
    tooltip.textContent = tips.join('\n\n')
    tooltip.hidden = false
    const maxX = scroll.clientWidth - tooltip.offsetWidth - 4
    tooltip.style.left = `${Math.max(0, Math.min(x + 12, maxX))}px`
    tooltip.style.top = `${y + 18}px`
})
scroll.addEventListener('mouseleave', () => {
    tooltip.hidden = true
    tipAt = ''
})

scroll.addEventListener('keydown', e => {
    if (editing) return
    const page = Math.max(Math.floor((scroll.clientHeight - headHeight()) / ROW_H) - 1, 1)
    const mod = e.ctrlKey || e.metaKey
    const moves = /** @type {Record<string, [number, number]>} */ ({
        ArrowUp: [-1, 0],
        ArrowDown: [1, 0],
        ArrowLeft: [0, -1],
        ArrowRight: [0, 1],
        PageUp: [-page, 0],
        PageDown: [page, 0],
    })
    if (moves[e.key]) {
        const [dr, dc] = moves[e.key]
        if (mod) move(dr * 1e9, dc * 1e9)
        else move(dr, dc)
    } else if (e.key === 'Home') {
        if (mod) move(-1e9, 0)
        select(state.row, 1)
    } else if (e.key === 'End') {
        if (mod) move(1e9, 0)
        select(state.row, Math.max(cols, 1))
    } else if (e.key === 'Tab') {
        move(0, e.shiftKey ? -1 : 1)
    } else if (e.key === 'Enter' || e.key === 'F2') {
        startEdit(null)
    } else if (e.key === 'Delete' || e.key === 'Backspace') {
        write(state.row, state.col, '')
        draw()
    } else if (e.key.length === 1 && !mod && !e.altKey) {
        startEdit(e.key)
    } else {
        return
    }
    e.preventDefault()
})
scroll.addEventListener('focus', draw)
scroll.addEventListener('blur', draw)

document.addEventListener('copy', e => {
    if (editing || document.activeElement === search) return
    const t = cell(state.row, state.col)
    e.clipboardData?.setData('text/plain', t ? t.text : '')
    e.preventDefault()
})

document.addEventListener('paste', e => {
    if (editing || document.activeElement === search) return
    const text = (e.clipboardData?.getData('text/plain') ?? '').replace(/\r?\n$/, '')
    // A block from a spreadsheet: tab-separated columns, one row per line.
    const list = allRows()
    const start = Math.max(list.indexOf(state.row), 0)
    text.split(/\r?\n/).forEach((line, i) => {
        const r = list[start + i]
        if (r !== undefined) line.split('\t').forEach((value, j) => write(r, state.col + j, value))
    })
    e.preventDefault()
    draw()
})

scroll.addEventListener('scroll', () => {
    tooltip.hidden = true
    placeEditor()
    draw()
})
new ResizeObserver(() => draw()).observe(area)
// The theme's colors and fonts, again when it changes.
new MutationObserver(() => {
    readTheme()
    measure()
    draw()
}).observe(document.body, {attributes: true, attributeFilter: ['class']})

// Toolbar

search.value = state.filter
search.addEventListener('input', () => {
    state.filter = search.value
    save()
    filter()
    scroll.scrollTop = 0
    info()
    draw()
})
search.addEventListener('keydown', e => {
    if (e.key === 'Enter' || e.key === 'ArrowDown') {
        e.preventDefault()
        if (order.length) select(order[0], state.col)
        scroll.focus()
    } else if (e.key === 'Escape') {
        search.value = ''
        search.dispatchEvent(new Event('input'))
    }
})
headerBox.checked = state.header
headerBox.addEventListener('change', () => {
    state.header = headerBox.checked
    save()
    filter()
    info()
    draw()
})
freezeBox.checked = state.freeze
freezeBox.addEventListener('change', () => {
    state.freeze = freezeBox.checked
    save()
    draw()
})
$('open-text').addEventListener('click', () => vscode.postMessage({type: 'openText'}))

// The sheet

window.addEventListener('message', e => {
    const msg = e.data
    if (msg.type !== 'table') return
    const first = !loaded
    const table = msg.table
    cells = new Map()
    for (const [r, c, kind, text, comment, formula] of table.cells) cells.set(key(r, c), {kind, text, comment, formula})
    const resized = table.cols !== cols || table.rows !== rows || first
    rows = table.rows
    cols = table.cols
    errors = table.errors
    loaded = true
    if (resized) measure()
    filter()
    info()
    $('loading').hidden = true
    if (first) {
        reveal()
        scroll.focus()
    }
    draw()
})

readTheme()
vscode.postMessage({type: 'ready'})
