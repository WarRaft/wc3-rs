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
 * Clicking a parameter opens World Editor's dialog for it: a preset, a
 * variable, a function (a call, with its default parameters) or a value,
 * of the parameter's type (`triggers::choices`); what's picked is written
 * at once, and a call's own parameters are links in turn. **New** (+)
 * picks a function for a new event, condition or action
 * (`triggers::catalog`). **New variable** declares a global in this
 * file (`[variable]`): name, type, array and size, initial value.
 *
 * Icons are World Editor's (`{type: 'icons', icons}`, once): a line's
 * category's, the headings' and blocks', the variables'.
 *
 * Messages in: `{type: 'view', view, select}` (select: a line to select,
 * 0 for none), `{type: 'select', line}`, `{type: 'choices', line, path,
 * choices}`, `{type: 'catalog', kind, place, items}`.
 * Messages out: `{type: 'ready'}`, `{type: 'toggle' | 'delete' | 'copy' |
 * 'cut', line}`, `{type: 'move', line, place}`, `{type: 'paste', place}`,
 * `{type: 'field', key, value}`, `{type: 'reveal', line}`,
 * `{type: 'choices', line, path}`, `{type: 'set', line, path, kind,
 * value}`, `{type: 'catalog', kind, place}`, `{type: 'new', place, kind,
 * name}`, `{type: 'openText'}`.
 */

// @ts-ignore: provided by VS Code
const vscode = acquireVsCodeApi()

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id))

/** @typedef {string | {r: string, p: string, s: Span[]}} Span */
/** @typedef {{label: string, group: number, kind: number, nodes: Node[]}} Block */
/** @typedef {{line: number, kind: number, enabled: boolean, name: string, known: boolean, icon: string, hint: string, text: Span[], blocks: Block[]}} Node */
/** @typedef {{line: number, scope: string, name: string, type: string, type_name: string, array: boolean, size: number, initial: string | null}} Variable */
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
/** What a variable's scope means. */
const SCOPE_TITLE = /** @type {Record<string, string>} */ ({
    global: 'seen by every trigger of the map',
    private: 'a global seen by this file only',
    local: "a local of this trigger's actions",
})
/** The editor's icons by kind (`[WorldEditArt]` keys). */
const KIND_ART = ['SEIcon_Event', 'SEIcon_Condition', 'SEIcon_Action']
/**
 * World Editor's icons, as `data:` URLs: each category's by its path, the
 * editor's own by their `[WorldEditArt]` keys (`triggers::icons`). Empty
 * without the game's files: then FontAwesome's stand in.
 * @type {Record<string, string>}
 */
let icons = {}
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
    // The trigger, always: a new file's or one of variables alone gets
    // its [trigger] with the first thing put in it.
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
        if (s.p) {
            el.dataset.path = s.p
            el.classList.add('link')
        }
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

    let icon = document.createElement('i')
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
        if (va.scope !== 'global') label.append(' ', h('span', {class: `scope scope-${va.scope}`}, [va.scope]))
        if (va.initial !== null && va.initial !== '') {
            const init = document.createElement('span')
            init.className = 'v v-l'
            init.textContent = va.initial
            label.append(' = ', init)
        }
        el.title = `${va.type}, ${SCOPE_TITLE[va.scope] || va.scope}\nLine ${va.line}`
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
    // World Editor's own icon over FontAwesome's, when the game's files
    // have it.
    const art = artOf(row)
    if (art && icons[art]) {
        const img = /** @type {HTMLImageElement} */ (document.createElement('img'))
        img.className = 'icon art'
        img.src = icons[art]
        img.alt = ''
        icon = /** @type {any} */ (img)
    }
    el.appendChild(icon)
    el.appendChild(label)
    return el
}

/**
 * The key of World Editor's icon for a row: a line's category's (or the
 * function icon for one the files don't have), a heading's and a block's
 * kind's, the variables'.
 * @param {Row} row
 */
function artOf(row) {
    if (row.type === 'node' && row.node) return row.node.known && row.node.icon ? row.node.icon : 'SEIcon_Function'
    if (row.type === 'heading') return KIND_ART[row.kind]
    if (row.type === 'block') return KIND_ART[row.kind]
    if (row.type === 'variables' || row.type === 'variable') return 'ToolBarIcon_SE_Variables'
    return ''
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
    set('delete', !!node || (!!row && row.type === 'variable'))
    set('copy', !!node)
    set('cut', !!node)
    set('up', !!node && node.index > 0)
    set('down', !!node && node.index < node.count - 1)
    set('paste', !!view)
    set('new', !!view && !!row && row.kind >= 0)
    set('new-variable', !!view)
}

/** @param {'toggle' | 'delete' | 'copy' | 'cut' | 'up' | 'down' | 'paste'} what */
function act(what) {
    const row = selectedRow()
    if (what === 'paste') return vscode.postMessage({type: 'paste', place: pastePlace()})
    if (what === 'delete' && row && row.type === 'variable' && row.variable) {
        return vscode.postMessage({type: 'delete', line: row.variable.line})
    }
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

$('new').addEventListener('click', () => newLine())
$('new-variable').addEventListener('click', () => vscode.postMessage({type: 'types'}))

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

tree.addEventListener('click', e => {
    const link = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.link'))
    const el = link && /** @type {HTMLElement | null} */ (link.closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    if (!link || !row || !row.node || !link.dataset.path) return
    vscode.postMessage({type: 'choices', line: row.node.line, path: link.dataset.path})
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
    else if (mod && e.shiftKey && key.toLowerCase() === 'n') newLine()
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
    const art = icons[v.enabled !== 0 ? 'SEIcon_Trigger' : 'SEIcon_TriggerDisabled']
    $('title-icon').innerHTML = art ? `<img class="icon art" alt="" src="${art}">` : '<i class="fa-solid fa-bolt"></i>'

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

// ---------------------------------------------------------------- dialogs

const overlay = $('dialog')
const box = /** @type {HTMLFormElement} */ ($('dialog-box'))

/** @param {string} tag @param {Record<string, any>} [props] @param {(Node | string)[]} [children] */
function h(tag, props = {}, children = []) {
    const el = /** @type {any} */ (document.createElement(tag))
    for (const [k, v] of Object.entries(props)) {
        if (k === 'class') el.className = v
        else if (k.startsWith('on')) el.addEventListener(k.slice(2), v)
        else if (k in el) el[k] = v
        else el.setAttribute(k, v)
    }
    el.append(...children)
    return el
}

function closeDialog() {
    overlay.hidden = true
    box.replaceChildren()
    box.onsubmit = null
    tree.focus()
}

/**
 * Shows the dialog with `body`; `ok` runs on OK (Enter), and closes it
 * unless it returns false.
 * @param {string} title @param {HTMLElement[]} body @param {() => boolean | void} ok
 */
function openDialog(title, body, ok) {
    box.replaceChildren(
        h('div', {class: 'dialog-title'}, [title]),
        ...body,
        h('div', {class: 'dialog-buttons'}, [
            h('button', {type: 'button', class: 'text-button', onclick: closeDialog}, ['Cancel']),
            h('button', {type: 'submit', class: 'primary-button'}, ['OK']),
        ]),
    )
    box.onsubmit = e => {
        e.preventDefault()
        if (ok() !== false) closeDialog()
    }
    overlay.hidden = false
}

overlay.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
        e.preventDefault()
        closeDialog()
    }
})
overlay.addEventListener('mousedown', e => {
    if (e.target === overlay) closeDialog()
})

/**
 * A list to pick from, filtered by what's typed: `items` are [value, label].
 * @param {[string, string][]} items @param {string} current @param {() => void} onPick
 */
function pickList(items, current, onPick) {
    const filter = h('input', {type: 'search', class: 'filter', placeholder: 'Filter', spellcheck: false})
    const list = /** @type {HTMLSelectElement} */ (h('select', {size: 10, class: 'list'}))
    const fill = () => {
        const q = filter.value.trim().toLowerCase()
        const keep = list.value || current
        list.replaceChildren(...items
            .filter(([v, l]) => !q || l.toLowerCase().includes(q) || v.toLowerCase().includes(q))
            .map(([v, l]) => h('option', {value: v, title: v}, [l])))
        if ([...list.options].some(o => o.value === keep)) list.value = keep
        else if (list.options.length) list.selectedIndex = 0
    }
    filter.addEventListener('input', fill)
    filter.addEventListener('keydown', e => {
        if (e.key === 'ArrowDown') {
            e.preventDefault()
            list.focus()
        }
    })
    list.addEventListener('dblclick', onPick)
    fill()
    return {filter, list}
}

/**
 * The dialog of a parameter, as World Editor's: preset, variable,
 * function or value, whichever its type has.
 * @param {number} line @param {string} path
 * @param {{type: string, type_name: string, current: {kind: string, value: string}, presets: [string, string][],
 *   variables: [string, string, boolean, string][], calls: [string, string, string][], value: boolean}} c
 */
function paramDialog(line, path, c) {
    /** @type {{kind: string, radio: HTMLInputElement, get: () => [string, string] | null}[]} */
    const options = []
    const body = []
    const option = (/** @type {string} */ kind, /** @type {string} */ label, /** @type {HTMLElement[]} */ controls, /** @type {() => [string, string] | null} */ get) => {
        const radio = /** @type {HTMLInputElement} */ (h('input', {type: 'radio', name: 'choice', value: kind}))
        radio.checked = c.current.kind === kind || (kind === 'variable' && c.current.kind === 'array')
        const section = h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, [radio, label]), ...controls])
        for (const el of controls) el.addEventListener('focusin', () => { radio.checked = true })
        options.push({kind, radio, get})
        body.push(section)
    }
    const submit = () => box.requestSubmit()
    if (c.presets.length) {
        const select = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'}, c.presets.map(([k, d]) => h('option', {value: k}, [d]))))
        if (c.current.kind === 'preset') select.value = c.current.value
        option('preset', 'Preset', [select], () => ['preset', select.value])
    }
    if (c.variables.length) {
        const select = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'},
            c.variables.map(([n, t, a, sc]) => h('option', {value: n}, [`${n}${a ? '[]' : ''} : ${t}${sc ? ` (${sc})` : ''}`]))))
        if (c.current.kind === 'variable') select.value = c.current.value
        option('variable', 'Variable', [select], () => {
            const v = c.variables.find(x => x[0] === select.value)
            return v ? [v[2] ? 'array' : 'variable', v[0]] : null
        })
    }
    if (c.calls.length) {
        const {filter, list} = pickList(c.calls.map(([n, t, cat]) => [n, cat ? `${cat} - ${t}` : t]),
            c.current.kind === 'call' ? c.current.value : '', submit)
        option('call', 'Function', [filter, list], () => list.value ? ['call', list.value] : null)
    }
    if (c.value) {
        const input = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'wide', spellcheck: false}))
        if (c.current.kind === 'value') input.value = c.current.value
        option('value', 'Value', [input], () => ['value', input.value])
    }
    if (!options.length) {
        body.push(h('div', {class: 'empty'}, [`Nothing of type ${c.type_name || c.type || '?'} to pick: open the text to write it.`]))
    }
    if (options.length && !options.some(o => o.radio.checked)) options[0].radio.checked = true
    openDialog(c.type_name || c.type || 'Parameter', body, () => {
        const chosen = options.find(o => o.radio.checked)
        const got = chosen && chosen.get()
        if (!got) return false
        vscode.postMessage({type: 'set', line, path, kind: got[0], value: got[1]})
    })
    const focus = options.find(o => o.radio.checked)
    const target = focus && /** @type {HTMLElement | null} */ (focus.radio.closest('.choice')?.querySelector('select, input:not([type=radio])'))
    if (target) target.focus()
}

/** New: asks the server for the functions of the selected row's kind. */
function newLine() {
    const row = selectedRow()
    if (!view || !row || row.kind < 0) return
    vscode.postMessage({type: 'catalog', kind: row.kind, place: pastePlace()})
}

/**
 * The dialog of a new line: a category, then a function in it.
 * @param {number} kind @param {string} place @param {[string, string, string, string, boolean][]} items
 */
function newDialog(kind, place, items) {
    const categories = [...new Set(items.map(i => i[2]))]
    const category = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'},
        [h('option', {value: '\u0000'}, ['- All -']), ...categories.map(cat => h('option', {value: cat}, [cat || '(none)']))]))
    const hint = h('div', {class: 'hint'})
    let picker = pickList([], '', () => box.requestSubmit())
    const holder = h('div', {class: 'picker'})
    const fill = () => {
        const all = category.value === '\u0000'
        const shown = items.filter(i => all || i[2] === category.value)
        const q = picker.filter.value
        picker = pickList(shown.map(([n, t, cat, , show]) => [n, all && cat && show ? `${cat} - ${t}` : t]), '', () => box.requestSubmit())
        picker.filter.value = q
        picker.filter.dispatchEvent(new Event('input'))
        picker.list.addEventListener('change', showHint)
        holder.replaceChildren(picker.filter, picker.list)
        showHint()
    }
    const showHint = () => {
        const it = items.find(i => i[0] === picker.list.value)
        hint.textContent = it ? it[3] : ''
    }
    category.addEventListener('change', fill)
    fill()
    const title = ['New Event', 'New Condition', 'New Action'][kind] || 'New'
    openDialog(title, [h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, ['Category']), category]), holder, hint], () => {
        if (!picker.list.value) return false
        vscode.postMessage({type: 'new', place, kind, name: picker.list.value})
    })
    picker.filter.focus()
}

/**
 * The dialog of a new global variable: name, type, array and size,
 * initial value.
 * @param {[string, string][]} types
 */
function variableDialog(types) {
    const name = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'wide', spellcheck: false, placeholder: 'name'}))
    const type = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'}, types.map(([t, n]) => h('option', {value: t}, [n]))))
    const integer = types.find(t => t[0] === 'integer')
    if (integer) type.value = integer[0]
    const array = /** @type {HTMLInputElement} */ (h('input', {type: 'checkbox'}))
    const size = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'size', value: '1', disabled: true}))
    array.addEventListener('change', () => { size.disabled = !array.checked })
    const initial = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'wide', spellcheck: false, placeholder: 'none'}))
    const scope = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'}, [
        h('option', {value: 'global'}, ['Global: every trigger of the map']),
        h('option', {value: 'private'}, ['Private: a global this file only sees']),
        h('option', {value: 'local'}, ["Local: this trigger's actions"]),
    ]))
    const row = (/** @type {string} */ label, /** @type {HTMLElement[]} */ controls) =>
        h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, [label]), ...controls])
    openDialog('New Variable', [
        row('Name', [name]),
        row('Type', [type]),
        row('Scope', [scope]),
        h('label', {class: 'check'}, [array, 'Array, of size ', size]),
        row('Initial value', [initial]),
    ], () => {
        if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(name.value.trim())) {
            name.focus()
            return false
        }
        vscode.postMessage({
            type: 'newVariable', name: name.value.trim(), vtype: type.value, array: array.checked,
            size: size.value.trim() || '1', scope: scope.value, initial: initial.value,
        })
    })
    name.focus()
}

// ---------------------------------------------------------------- messages

window.addEventListener('message', e => {
    const msg = e.data
    if (msg.type === 'view') {
        view = msg.view
        if (msg.select) selectLine = msg.select
        showFields(msg.view)
        draw()
    } else if (msg.type === 'choices') {
        paramDialog(msg.line, msg.path, msg.choices)
    } else if (msg.type === 'icons') {
        icons = msg.icons || {}
        draw()
    } else if (msg.type === 'types') {
        variableDialog(msg.items)
    } else if (msg.type === 'catalog') {
        newDialog(msg.kind, msg.place, msg.items)
    } else if (msg.type === 'select') {
        const r = rows.find(r => r.type === 'node' && r.node && r.node.line === msg.line)
        if (r) select(r.path)
    }
})

vscode.postMessage({type: 'ready'})
tree.focus()
