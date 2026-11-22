// @ts-check
'use strict'

/**
 * A trigger as World Editor shows it, a webview page (`extension.js`,
 * `registerTrigger`). The text stays the document: the extension posts
 * the view the server reads from it (`triggers::view`), and this page asks
 * for operations (turn on or off, delete, move, copy, cut, paste, set a
 * field); the extension has the server turn them into text edits and
 * applies them, then posts the new view.
 *
 * The tree is one list of rows, indented: the three headings (Events,
 * Conditions, Actions), each line (an event, condition or action), and
 * each block of a line (an if/then/else's If - Conditions, Then - Actions,
 * Else - Actions; a loop's Loop - Actions) with what's inside it. A row
 * collapses by its path in the tree, so it stays collapsed while the text
 * changes around it.
 *
 * A place in the tree, as the server takes it, is `parent group index`:
 * the line of the line it's under (0 at the top), its block (at the top,
 * the kind: 0 events, 1 conditions, 2 actions), before what's at `index`.
 *
 * Messages in: `{type: 'view', view, select}` (select: a line to select,
 * 0 for none), `{type: 'select', line}`.
 * Messages out: `{type: 'ready'}`, `{type: 'toggle' | 'delete' | 'copy' |
 * 'cut', line}`, `{type: 'move', line, place}`, `{type: 'paste', place}`,
 * `{type: 'field', key, value}`, `{type: 'reveal', line}`,
 * `{type: 'openText'}`.
 */

// @ts-ignore: provided by VS Code
const vscode = acquireVsCodeApi()

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id))

/** @typedef {string | {r: string, s: Span[]}} Span */
/** @typedef {{label: string, group: number, kind: number, nodes: Node[]}} Block */
/** @typedef {{line: number, kind: number, enabled: boolean, name: string, known: boolean, icon: string, hint: string, text: Span[], blocks: Block[]}} Node */
/** @typedef {{line: number, name: string, type: string, type_name: string, array: boolean, size: number, initial: string | null}} Variable */
/**
 * @typedef {{
 *   trigger: boolean, variables: Variable[],
 *   name: string, description: string, enabled: number, initially_off: number, run_on_init: number,
 *   custom_text: number, comment: number, data: boolean, errors: [number, string][], labels: string[],
 *   events: Node[], conditions: Node[], actions: Node[]
 * }} View
 */
/**
 * A row of the tree.
 * @typedef {{
 *   type: 'heading' | 'node' | 'block' | 'variable' | 'variables', variable?: Variable, path: string, depth: number, label: string,
 *   kind: number, parent: number, group: number, index: number, count: number,
 *   node?: Node, off: boolean, inOff: boolean, open: boolean, hasChildren: boolean,
 * }} Row
 */

const KIND_CLASS = ['event', 'condition', 'action']
const KIND_ICON = ['fa-bolt', 'fa-scale-balanced', 'fa-play']
const INDENT = 18

/** @type {View | null} */
let view = null
/** @type {Row[]} */
let rows = []
const saved = vscode.getState() || {}
/** Paths of collapsed rows. */
const collapsed = new Set(saved.collapsed || [])
/** The selected row's path. */
let selected = saved.selected || ''
/** A line to select once a view has it (after an operation). */
let selectLine = 0

const tree = $('tree')
const save = () => vscode.setState({collapsed: [...collapsed], selected})

// ---------------------------------------------------------------- rows

/** @param {View} v */
function buildRows(v) {
    /** @type {Row[]} */
    const out = []
    if (v.variables.length) {
        const path = 'vars'
        const open = !collapsed.has(path)
        out.push({
            type: 'variables', path, depth: 0, label: v.labels[3] || 'Variables', kind: -1,
            parent: 0, group: -1, index: 0, count: v.variables.length, off: false, inOff: false, open, hasChildren: true,
        })
        if (open) {
            v.variables.forEach((variable, index) => out.push({
                type: 'variable', path: `vars/${index}`, depth: 1, label: '', kind: -1, parent: 0, group: -1, index,
                count: v.variables.length, variable, off: false, inOff: false, open: false, hasChildren: false,
            }))
        }
    }
    if (!v.trigger) return out
    const lists = [v.events, v.conditions, v.actions]
    lists.forEach((nodes, kind) => {
        const path = `h${kind}`
        const open = !collapsed.has(path)
        out.push({
            type: 'heading', path, depth: 0, label: v.labels[kind] || KIND_CLASS[kind], kind,
            parent: 0, group: kind, index: 0, count: nodes.length, off: false, inOff: false, open, hasChildren: nodes.length > 0,
        })
        if (open) addNodes(out, nodes, path, 1, 0, kind, false)
    })
    return out
}

/**
 * @param {Row[]} out
 * @param {Node[]} nodes
 * @param {string} base
 * @param {number} depth
 * @param {number} parent
 * @param {number} group
 * @param {boolean} inOff
 */
function addNodes(out, nodes, base, depth, parent, group, inOff) {
    nodes.forEach((node, index) => {
        const path = `${base}/${index}`
        const open = !collapsed.has(path)
        const off = !node.enabled
        out.push({
            type: 'node', path, depth, label: '', kind: node.kind, parent, group, index, count: nodes.length, node,
            off, inOff, open, hasChildren: node.blocks.length > 0,
        })
        if (!open) return
        node.blocks.forEach(block => {
            const bpath = `${path}/b${block.group}`
            const bopen = !collapsed.has(bpath)
            out.push({
                type: 'block', path: bpath, depth: depth + 1, label: block.label, kind: block.kind, parent: node.line,
                group: block.group, index: 0, count: block.nodes.length, off: false, inOff: inOff || off, open: bopen,
                hasChildren: block.nodes.length > 0,
            })
            if (bopen) addNodes(out, block.nodes, bpath, depth + 2, node.line, block.group, inOff || off)
        })
    })
}

// ---------------------------------------------------------------- drawing

/**
 * @param {HTMLElement} into
 * @param {Span[]} spans
 */
function drawSpans(into, spans) {
    for (const s of spans) {
        if (typeof s === 'string') {
            into.appendChild(document.createTextNode(s))
            continue
        }
        const el = document.createElement('span')
        el.className = `v v-${s.r}`
        drawSpans(el, s.s)
        into.appendChild(el)
    }
}

function draw() {
    if (!view) return
    const v = view
    rows = buildRows(v)
    if (selectLine) {
        const r = rows.find(r => r.type === 'node' && r.node && r.node.line === selectLine)
        if (r) selected = r.path
        selectLine = 0
    }
    if (!rows.some(r => r.path === selected)) selected = rows.length ? rows[0].path : ''
    const frag = document.createDocumentFragment()
    for (const row of rows) frag.appendChild(drawRow(row))
    tree.replaceChildren(frag)
    $('loading').hidden = true
    updateTools()
    save()
}

/** @param {Row} row */
function drawRow(row) {
    const el = document.createElement('div')
    el.className = `row ${row.type}-row`
    if (row.type === 'heading' || row.type === 'variables') el.classList.add('heading')
    if (row.off) el.classList.add('off')
    if (row.inOff) el.classList.add('in-off')
    if (row.path === selected) el.classList.add('selected')
    el.dataset.path = row.path
    el.style.paddingLeft = `${8 + row.depth * INDENT}px`
    el.setAttribute('role', 'treeitem')

    const twisty = document.createElement('span')
    twisty.className = 'twisty'
    if (row.hasChildren || (row.type !== 'node' && row.type !== 'variable')) {
        twisty.innerHTML = `<i class="fa-solid ${row.open ? 'fa-chevron-down' : 'fa-chevron-right'}"></i>`
        if (!row.hasChildren) twisty.style.visibility = 'hidden'
        twisty.addEventListener('click', e => {
            e.stopPropagation()
            setOpen(row, !row.open)
        })
    }
    el.appendChild(twisty)

    const icon = document.createElement('i')
    const label = document.createElement('span')
    label.className = 'label'
    if (row.type === 'node' && row.node) {
        const n = row.node
        icon.className = `icon fa-solid ${n.known ? KIND_ICON[n.kind] : 'fa-triangle-exclamation'} ${n.known ? KIND_CLASS[n.kind] : 'unknown'}`
        drawSpans(label, n.text)
        el.title = [n.name, n.hint, n.known ? '' : 'Not in UI\\TriggerData.txt', `Line ${n.line}`].filter(Boolean).join('\n')
        el.draggable = true
    } else if (row.type === 'variable' && row.variable) {
        const va = row.variable
        icon.className = 'icon fa-solid fa-cube variable'
        const name = document.createElement('span')
        name.className = 'v v-v'
        name.textContent = va.name
        label.append(name, ` : ${va.type_name}${va.array ? ` [${va.size}]` : ''}`)
        if (va.initial !== null && va.initial !== '') {
            const init = document.createElement('span')
            init.className = 'v v-l'
            init.textContent = va.initial
            label.append(' = ', init)
        }
        el.title = `${va.type}\nLine ${va.line}`
    } else if (row.type === 'variables') {
        icon.className = 'icon fa-solid fa-cubes variable'
        label.textContent = row.label
    } else if (row.type === 'heading') {
        icon.className = `icon fa-solid ${KIND_ICON[row.kind]} ${KIND_CLASS[row.kind]}`
        label.textContent = row.label
    } else {
        icon.className = `icon fa-solid ${row.open && row.hasChildren ? 'fa-folder-open' : 'fa-folder'} block`
        label.textContent = row.label
    }
    el.appendChild(icon)
    el.appendChild(label)
    return el
}

/**
 * @param {Row} row
 * @param {boolean} open
 */
function setOpen(row, open) {
    if (open) collapsed.delete(row.path)
    else collapsed.add(row.path)
    draw()
}

function selectedRow() {
    return rows.find(r => r.path === selected) || null
}

/** @param {string} path */
function select(path) {
    selected = path
    for (const el of tree.querySelectorAll('.row.selected')) el.classList.remove('selected')
    const el = tree.querySelector(`.row[data-path="${CSS.escape(path)}"]`)
    if (el) {
        el.classList.add('selected')
        el.scrollIntoView({block: 'nearest'})
    }
    updateTools()
    save()
}

// ---------------------------------------------------------------- operations

/** Where a paste goes for the selected row: after a line, at a block's or heading's end. */
function pastePlace() {
    const row = selectedRow()
    if (!row || row.group < 0) return '0 2 99999'
    if (row.type === 'node') return `${row.parent} ${row.group} ${row.index + 1}`
    return `${row.parent} ${row.group} ${row.count}`
}

function updateTools() {
    const row = selectedRow()
    const node = row && row.type === 'node' ? row : null
    const set = (/** @type {string} */ id, /** @type {boolean} */ on) => { /** @type {HTMLButtonElement} */ ($(id)).disabled = !on }
    set('toggle', !!node)
    set('delete', !!node)
    set('copy', !!node)
    set('cut', !!node)
    set('up', !!node && node.index > 0)
    set('down', !!node && node.index < node.count - 1)
    set('paste', !!view && view.trigger)
}

/** @param {'toggle' | 'delete' | 'copy' | 'cut' | 'up' | 'down' | 'paste'} what */
function act(what) {
    const row = selectedRow()
    if (what === 'paste') return vscode.postMessage({type: 'paste', place: pastePlace()})
    if (!row || row.type !== 'node' || !row.node) return
    const line = row.node.line
    if (what === 'up' || what === 'down') {
        if (what === 'up' && row.index === 0) return
        if (what === 'down' && row.index >= row.count - 1) return
        const index = what === 'up' ? row.index - 1 : row.index + 2
        return vscode.postMessage({type: 'move', line, place: `${row.parent} ${row.group} ${index}`})
    }
    vscode.postMessage({type: what, line})
}

for (const id of ['toggle', 'delete', 'copy', 'cut', 'up', 'down', 'paste']) {
    $(id).addEventListener('click', () => {
        act(/** @type {any} */ (id))
        tree.focus()
    })
}

// ---------------------------------------------------------------- mouse and keys

tree.addEventListener('mousedown', e => {
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    if (el && el.dataset.path) select(el.dataset.path)
})

tree.addEventListener('dblclick', e => {
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    if (!row) return
    if (row.type === 'node' && row.node) vscode.postMessage({type: 'reveal', line: row.node.line})
    else if (row.type === 'variable' && row.variable) vscode.postMessage({type: 'reveal', line: row.variable.line})
    else setOpen(row, !row.open)
})

tree.addEventListener('keydown', e => {
    const row = selectedRow()
    const i = row ? rows.indexOf(row) : -1
    const mod = e.ctrlKey || e.metaKey
    const key = e.key
    let handled = true
    if (e.altKey && key === 'ArrowUp') act('up')
    else if (e.altKey && key === 'ArrowDown') act('down')
    else if (key === 'ArrowUp') { if (i > 0) select(rows[i - 1].path) }
    else if (key === 'ArrowDown') { if (i < rows.length - 1) select(rows[i + 1].path) }
    else if (key === 'Home') { if (rows.length) select(rows[0].path) }
    else if (key === 'End') { if (rows.length) select(rows[rows.length - 1].path) }
    else if (key === 'ArrowLeft' && row) {
        if (row.open && row.hasChildren) setOpen(row, false)
        else {
            // To the row it's in.
            const up = row.path.slice(0, row.path.lastIndexOf('/'))
            if (up) select(up)
        }
    } else if (key === 'ArrowRight' && row) {
        if (!row.open && row.hasChildren) setOpen(row, true)
        else if (rows[i + 1] && rows[i + 1].depth > row.depth) select(rows[i + 1].path)
    } else if (key === 'Enter' && row && (row.node || row.variable)) {
        vscode.postMessage({type: 'reveal', line: row.node ? row.node.line : row.variable ? row.variable.line : 0})
    }
    else if (key === 'Delete' || (key === 'Backspace' && e.metaKey)) act('delete')
    else if (mod && key.toLowerCase() === 'c') act('copy')
    else if (mod && key.toLowerCase() === 'x') act('cut')
    else if (mod && key.toLowerCase() === 'v') act('paste')
    else if (mod && key.toLowerCase() === 'e') act('toggle')
    else handled = false
    if (handled) e.preventDefault()
})

// Drag and drop: a line onto another goes before or after it; onto a
// heading or a block, at its end.

/** @type {Row | null} */
let dragged = null
/** @type {{row: Row, place: string, cls: string} | null} */
let dropAt = null

const clearDrop = () => {
    for (const el of tree.querySelectorAll('.drop-before, .drop-after, .drop-into')) el.classList.remove('drop-before', 'drop-after', 'drop-into')
}

tree.addEventListener('dragstart', e => {
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    if (!el || !row || row.type !== 'node' || !e.dataTransfer) return
    dragged = row
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', row.path)
    el.classList.add('dragging')
    select(row.path)
})

tree.addEventListener('dragend', () => {
    for (const el of tree.querySelectorAll('.dragging')) el.classList.remove('dragging')
    clearDrop()
    dragged = null
    dropAt = null
})

tree.addEventListener('dragover', e => {
    if (!dragged || !e.dataTransfer) return
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    clearDrop()
    dropAt = null
    if (!el || !row) return
    // Not into itself.
    if (row.path === dragged.path || row.path.startsWith(`${dragged.path}/`)) return
    if (row.group < 0) return
    if (row.type === 'node') {
        if (row.kind !== dragged.kind) return
        const box = el.getBoundingClientRect()
        const after = e.clientY > box.top + box.height / 2
        dropAt = {row, place: `${row.parent} ${row.group} ${row.index + (after ? 1 : 0)}`, cls: after ? 'drop-after' : 'drop-before'}
    } else {
        if (row.kind !== dragged.kind) return
        dropAt = {row, place: `${row.parent} ${row.group} ${row.count}`, cls: 'drop-into'}
    }
    el.classList.add(dropAt.cls)
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
})

tree.addEventListener('dragleave', e => {
    if (e.target === tree) clearDrop()
})

tree.addEventListener('drop', e => {
    e.preventDefault()
    clearDrop()
    if (dragged && dragged.node && dropAt) vscode.postMessage({type: 'move', line: dragged.node.line, place: dropAt.place})
    dragged = null
    dropAt = null
})

// ---------------------------------------------------------------- fields

const nameInput = /** @type {HTMLInputElement} */ ($('name'))
const description = /** @type {HTMLTextAreaElement} */ ($('description'))
const enabled = /** @type {HTMLInputElement} */ ($('enabled'))
const initiallyOn = /** @type {HTMLInputElement} */ ($('initially-on'))

const field = (/** @type {string} */ key, /** @type {string} */ value) => vscode.postMessage({type: 'field', key, value})

nameInput.addEventListener('change', () => {
    if (view && nameInput.value !== view.name) field('name', nameInput.value)
})
nameInput.addEventListener('keydown', e => {
    if (e.key === 'Enter') nameInput.blur()
    if (e.key === 'Escape' && view) {
        nameInput.value = view.name
        nameInput.blur()
    }
})
description.addEventListener('change', () => {
    if (view && description.value !== view.description) field('description', description.value)
})
enabled.addEventListener('change', () => field('enabled', enabled.checked ? '1' : '0'))
initiallyOn.addEventListener('change', () => field('initially_off', initiallyOn.checked ? '0' : '1'))

$('problems').addEventListener('click', () => {
    if (view && view.errors.length) vscode.postMessage({type: 'reveal', line: view.errors[0][0]})
})
$('open-text').addEventListener('click', () => vscode.postMessage({type: 'openText'}))

/** @param {View} v */
function showFields(v) {
    for (const id of ['name', 'description']) $(id).hidden = !v.trigger
    for (const el of document.querySelectorAll('.check')) /** @type {HTMLElement} */ (el).hidden = !v.trigger
    // What's being typed stays.
    if (document.activeElement !== nameInput) nameInput.value = v.name
    if (document.activeElement !== description) description.value = v.description
    enabled.checked = v.enabled !== 0
    initiallyOn.checked = v.initially_off === 0
    const problems = $('problems')
    problems.hidden = !v.errors.length
    problems.textContent = `${v.errors.length} problem${v.errors.length === 1 ? '' : 's'}`
    problems.title = v.errors.slice(0, 10).map(([line, m]) => `Line ${line}: ${m}`).join('\n')
    const notes = []
    if (!v.data) notes.push('No UI\\TriggerData.txt in the project\'s game folders or map roots: functions show as calls, without their phrases.')
    if (v.custom_text) notes.push('Custom text: the trigger is its code (custom_text=1); what\'s below is what it was before.')
    if (v.comment) notes.push('A comment (comment=1), not a trigger.')
    const notice = $('notice')
    notice.hidden = !notes.length
    notice.textContent = notes.join(' ')
}

// ---------------------------------------------------------------- messages

window.addEventListener('message', e => {
    const msg = e.data
    if (msg.type === 'view') {
        view = msg.view
        if (msg.select) selectLine = msg.select
        showFields(msg.view)
        draw()
    } else if (msg.type === 'select') {
        const r = rows.find(r => r.type === 'node' && r.node && r.node.line === msg.line)
        if (r) select(r.path)
    }
})

vscode.postMessage({type: 'ready'})
tree.focus()
