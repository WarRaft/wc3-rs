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
 * A double click on a line (a click on a parameter is the parameter's)
 * opens the same dialog as New, of that line: another
 * function replaces its own (`replace`); a double click on a variable opens New
 * variable's, filled in (`setVariable`). Both have Show in Text. What a
 * variable's change or deletion breaks the extension shows before it's
 * done (`triggers::op::CHECK_VARIABLE`).
 *
 * Shift with a click or an arrow selects the lines from the one selected
 * before, beside each other in its heading or block; Ctrl (Cmd on a Mac)
 * with a click adds a line or takes it out, anywhere. Copy, Cut, Delete,
 * Enable/Disable and dragging take them all (`lines`, the others'
 * lines); Paste goes after the last; Alt+Up/Down moves them when they're
 * in one heading or block.
 *
 * Icons are World Editor's (`{type: 'icons', icons}`, once): a line's
 * category's, the headings' and blocks', the variables'.
 *
 * Messages in: `{type: 'view', view, select}` (select: a line to select,
 * 0 for none), `{type: 'select', line}`, `{type: 'choices', line, path,
 * choices}`, `{type: 'catalog', kind, place, line, items}` (line: the
 * line edited, 0 for a new one), `{type: 'types', line, items}`.
 * Messages out: `{type: 'ready'}`, `{type: 'convert'}`, `{type: 'toggle' | 'delete' | 'copy' |
 * 'cut', line, lines}`, `{type: 'move', line, lines, place}`, `{type: 'paste', place}`,
 * `{type: 'field', key, value}`, `{type: 'reveal', line}`,
 * `{type: 'choices', line, path}`, `{type: 'set', line, path, kind,
 * value}`, `{type: 'catalog', kind, place, line}`, `{type: 'new', place, kind,
 * name}`, `{type: 'replace', line, name}`, `{type: 'types', line}`,
 * `{type: 'renamed', from}`,
 * `{type: 'deleteVariable', line}`,
 * `{type: 'newVariable' | 'setVariable', line, name, vtype, array, size,
 * scope, initial}`, `{type: 'openText'}`, `{type: 'show', language}` (`jass`, `lua`, `angelscript`).
 */

/**
 * One code for every place the trigger editor is in: its own tab
 * (its page mounts it on its root) and the World Editor's Triggers
 * panel (`editor/editor.js`). `mount(root, host)`: `root` holds its markup
 * (`trigger-markup.js`; with the class `wtg`, a map's tree beside it,
 * `TriggerEditorMap`), `host` is how it talks to the extension (`postMessage`,
 * `getState`, `setState`: the page's API, or the panel's stand-in); what
 * the extension posts goes to the `receive` it gives back. Everything it
 * looks up is in `root`, so it lives beside another page's elements.
 */
const TriggerEditor = (() => {
// ---------------------------------------------------------------- a map's tree
/**
 * The tree of a map's triggers (`war3map.wtg`), beside the trigger picked
 * in it (the trigger editor shows that one; `extension.js`, `registerTrigger`).
 * There are no files to show them: the server reads the tree from the
 * text (`triggers::wtg_view`) and this page draws it, as World Editor
 * does: each category with its triggers (those of a category the file
 * hasn't at the top), then the variables.
 *
 * A click selects a row; a trigger selected is the one picked, shown on
 * the right. The tools above the tree (and the keys) make a new category,
 * trigger or variable, rename (F2) or delete (Delete) what's selected:
 * a category's name in a dialog, a trigger's in its name field, a
 * variable in its dialog (the trigger's, without a scope: a map's variables
 * are globals). A double click on a category opens or closes it, on a
 * variable edits it. A category collapses by its index, kept while the
 * text changes. A trigger is moved by dragging it: onto a trigger, before
 * it (in its category); onto a category, to its end; Alt+Up and Alt+Down
 * move the selected one in its category.
 *
 * `mount` calls `TriggerEditorMap` with what of its own this
 * uses (`vscode`, its host; `h`, `openDialog`, its `root` and `$`, and
 * `setMapVariables`) and passes it what the extension posts.
 *
 * Messages in: `{type: 'tree', tree}` (`{categories, triggers, variables,
 * picked}`), `{type: 'icons', icons}`. Messages out: `{type: 'pick',
 * nth}` (which trigger, from 1: its place among the triggers, so typing
 * in the text above it doesn't pick another), `{type: 'revealText',
 * line}`, `{type: 'extract'}`, `{type: 'newCategory', name}`,
 * `{type: 'newTrigger', category, name}`, `{type: 'renameCategory',
 * index, name}`, `{type: 'deleteTrigger', name}`, `{type:
 * 'deleteCategory', index, name, count}`, `{type: 'deleteVariable',
 * line}`, `{type: 'types', line}` (a variable's dialog; 0 for a new one),
 * `{type: 'moveTrigger', moved, category, before}` (which, from 1; the
 * category's index; the trigger it goes before, 0 for the category's
 * end).
 */

function TriggerEditorMap(/** @type {any} */ {vscode: mapApi, h, openDialog, root, $, setMapVariables}) {

/** @typedef {{line: number, index: number, name: string, comment: number}} MapCategory */
/** @typedef {{line: number, name: string, category: number, enabled: number, comment: number, custom: number}} MapTrigger */
/** @typedef {{line: number, scope: string, name: string, type: string, type_name: string, array: boolean, size: number, initial: string | null}} MapVariable */
/** `picked`: which trigger, from 1 (0: none). */
/**
 * The map's custom script beside the triggers (`war3map.wct`): where its
 * header and each trigger's code are in its text, what's wrong.
 * @typedef {{header: [number, number] | null, code: [number, number, boolean][], problem: string | null}} MapScript
 */
/** @typedef {{categories: MapCategory[], triggers: MapTrigger[], variables: MapVariable[], picked: number, wct: MapScript | null}} MapTree */
/**
 * A row selected: a trigger by its place (from 1), a category by its
 * index, a variable by its line, the variables' heading, the map's
 * custom script.
 * @typedef {{kind: 'trigger' | 'category' | 'variable' | 'variables' | 'script', key: number}} MapSelection
 */

const mapTree = /** @type {HTMLElement} */ ($('map-tree'))
/** @type {MapTree | null} */
let mapData = null
/** @type {Record<string, string>} */
let mapIcons = {}
/** Collapsed categories, by index; `vars` the variables. */
const shut = new Set()
/** @type {MapSelection} */
let mapSelected = {kind: 'trigger', key: 0}
/** The rows as drawn, for the keys. @type {{sel: MapSelection, el: HTMLElement}[]} */
let mapRows = []

/**
 * @param {string} art a `[WorldEditArt]` key
 * @param {string} fa FontAwesome's stand-in
 * @param {string} cls
 */
function iconEl(art, fa, cls) {
    const el = document.createElement('span')
    el.className = `map-icon ${cls}`
    const src = mapIcons[art]
    if (src) {
        const img = document.createElement('img')
        img.className = 'icon art'
        img.alt = ''
        img.src = src
        el.append(img)
    } else {
        el.innerHTML = `<i class="fa-solid ${fa}"></i>`
    }
    return el
}

/** @param {MapSelection} a @param {MapSelection} b */
const same = (a, b) => a.kind === b.kind && a.key === b.key

/**
 * @param {MapSelection} sel
 * @param {number} depth
 * @param {string | null} twisty '▸', '▾' or none
 * @param {HTMLElement} icon
 * @param {string} label
 * @param {string} [detail]
 */
function rowEl(sel, depth, icon, label, detail) {
    const el = document.createElement('div')
    el.className = 'map-row'
    el.style.paddingLeft = `${8 + depth * 16}px`
    const l = document.createElement('span')
    l.className = 'map-label'
    l.textContent = label
    el.append(icon, l)
    if (detail) {
        const d = document.createElement('span')
        d.className = 'map-detail'
        d.textContent = detail
        el.append(d)
    }
    if (same(sel, mapSelected)) el.classList.add('picked')
    el.addEventListener('click', () => choose(sel))
    mapRows.push({sel, el})
    return el
}

/** Opens or closes a category, or the variables. @param {MapSelection} sel */
function toggle(sel) {
    const key = sel.kind === 'variables' ? 'vars' : sel.key
    if (shut.has(key)) shut.delete(key)
    else shut.add(key)
    drawTree()
}

/** The trigger being dragged (from 1), 0 for none. */
let dragging = 0

/** Moves the trigger `moved` into `category`, before `before` (0: at its end). @param {number} moved @param {number} category @param {number} before */
function moveTrigger(moved, category, before) {
    if (!moved || moved === before) return
    // It's picked where it goes: selected once the tree comes.
    mapSelected = {kind: 'trigger', key: 0}
    mapApi.postMessage({type: 'moveTrigger', moved, category, before})
}

/**
 * Lets `el` take a dragged trigger: `before` it (its place, from 1) in
 * `category`, or at the category's end (`before` 0).
 * @param {HTMLElement} el @param {number} category @param {number} before
 */
function dropTarget(el, category, before) {
    const cls = before ? 'drop-before' : 'drop-into'
    el.addEventListener('dragover', e => {
        if (!dragging || dragging === before) return
        e.preventDefault()
        if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
        el.classList.add(cls)
    })
    el.addEventListener('dragleave', () => el.classList.remove(cls))
    el.addEventListener('drop', e => {
        e.preventDefault()
        el.classList.remove(cls)
        const moved = dragging
        dragging = 0
        moveTrigger(moved, category, before)
    })
}

/** @param {MapTrigger} t @param {number} depth */
function triggerRow(t, depth) {
    const nth = (mapData ? mapData.triggers.indexOf(t) : -1) + 1
    // A comment is set apart: its own icon, its text muted.
    const art = t.comment ? '' : t.enabled ? 'SEIcon_Trigger' : 'SEIcon_TriggerDisabled'
    const icon = iconEl(art, t.comment ? 'fa-comment' : 'fa-bolt', t.comment ? 'comment' : 'trigger')
    const el = rowEl({kind: 'trigger', key: nth}, depth, icon, t.name || '(no name)', t.custom ? 'custom script' : '')
    el.draggable = true
    el.addEventListener('dragstart', e => {
        dragging = nth
        if (e.dataTransfer) {
            e.dataTransfer.effectAllowed = 'move'
            e.dataTransfer.setData('text/plain', t.name)
        }
        el.classList.add('dragging')
    })
    el.addEventListener('dragend', () => {
        dragging = 0
        el.classList.remove('dragging')
    })
    dropTarget(el, t.category, nth)
    if (!t.enabled) el.classList.add('off')
    if (t.comment) el.classList.add('comment')
    // Custom text: its code is in war3map.wct (double click: open it).
    if (t.custom) el.addEventListener('dblclick', () => mapApi.postMessage({type: 'openCode', nth}))
    el.title = [t.name, t.comment ? 'a comment' : '', t.enabled ? '' : 'disabled', t.custom ? 'custom script (war3map.wct)' : ''].filter(Boolean).join(' — ')
    return el
}

function drawTree() {
    mapTree.textContent = ''
    mapRows = []
    if (!mapData) return
    const data = mapData
    const frag = document.createDocumentFragment()
    // The variables first, then the map's custom script, then the categories.
    const varsOpen = !shut.has('vars')
    const vars = /** @type {MapSelection} */ ({kind: 'variables', key: 0})
    const heading = rowEl(vars, 0, iconEl('ToolBarIcon_SE_Variables', 'fa-cube', 'variable'), 'Variables', String(data.variables.length))
    heading.addEventListener('dblclick', () => toggle(vars))
    frag.append(heading)
    if (varsOpen) {
        for (const v of data.variables) {
            const type = v.array ? `${v.type_name} array` : v.type_name
            const row = rowEl({kind: 'variable', key: v.line}, 1, iconEl('', 'fa-cube', 'variable'), v.name, type)
            row.title = `${v.name}: ${type}${v.initial !== null ? ` = ${v.initial}` : ''} (double click: edit)`
            row.addEventListener('dblclick', () => mapApi.postMessage({type: 'types', line: v.line}))
            frag.append(row)
        }
    }
    // The map's custom script, at the top as World Editor has it.
    if (data.wct && data.wct.header) {
        const row = rowEl({kind: 'script', key: 0}, 0, iconEl('', 'fa-scroll', 'script'), 'Map Script', 'war3map.wct')
        row.title = "The map's custom script: code before every trigger's (opens war3map.wct)"
        frag.append(row)
    }
    const known = new Set(data.categories.map(c => c.index))
    for (const t of data.triggers.filter(t => !known.has(t.category))) frag.append(triggerRow(t, 0))
    for (const c of data.categories) {
        const open = !shut.has(c.index)
        const art = open ? 'SEIcon_TriggerCategoryOpen' : 'SEIcon_TriggerCategory'
        const sel = /** @type {MapSelection} */ ({kind: 'category', key: c.index})
        // Open or shut by double click (or Enter), its folder showing which.
        const el = rowEl(sel, 0, iconEl(art, open ? 'fa-folder-open' : 'fa-folder', 'category'), c.name || '(no name)')
        if (c.comment) el.classList.add('comment')
        el.addEventListener('dblclick', () => toggle(sel))
        dropTarget(el, c.index, 0)
        frag.append(el)
        if (open) for (const t of data.triggers.filter(t => t.category === c.index)) frag.append(triggerRow(t, 1))
    }
    mapTree.append(frag)
    mapTree.querySelector('.map-row.picked')?.scrollIntoView({block: 'nearest'})
    updateMapTools()
}

/** Selects a row; a trigger is picked too. @param {MapSelection} sel */
function choose(sel) {
    if (!mapData) return
    mapSelected = sel
    if (sel.kind === 'script') mapApi.postMessage({type: 'openCode', nth: 0})
    if (sel.kind === 'trigger' && sel.key !== mapData.picked) {
        mapData.picked = sel.key
        mapApi.postMessage({type: 'pick', nth: sel.key})
    }
    drawTree()
    mapTree.focus()
}

/** The category a new trigger goes in: the one selected, or the picked trigger's, or the first. */
function categoryForNew() {
    if (!mapData) return 0
    if (mapSelected.kind === 'category') return mapSelected.key
    const t = mapData.triggers[mapData.picked - 1]
    if (t) return t.category
    return mapData.categories.length ? mapData.categories[0].index : 0
}

/** A name like World Editor's (`Untitled Trigger 001`) that none has. @param {string} base @param {string[]} taken */
function freshName(base, taken) {
    const lower = new Set(taken.map(n => n.toLowerCase()))
    for (let i = 1; ; i++) {
        const name = `${base} ${String(i).padStart(3, '0')}`
        if (!lower.has(name.toLowerCase())) return name
    }
}

/** A dialog asking for a name. @param {string} title @param {string} value @param {(name: string) => void} done */
function nameDialog(title, value, done) {
    // @ts-ignore: mount's
    const input = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'wide', spellcheck: false, value}))
    // @ts-ignore: mount's
    openDialog(title, [h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, ['Name']), input])], () => {
        if (!input.value.trim()) {
            input.focus()
            return false
        }
        done(input.value.trim())
    })
    input.focus()
    input.select()
}

const mapActions = {
    newCategory() {
        if (!mapData) return
        nameDialog('New Category', freshName('Untitled Category', mapData.categories.map(c => c.name)),
            name => mapApi.postMessage({type: 'newCategory', name}))
    },
    newTrigger() {
        if (!mapData) return
        const category = categoryForNew()
        nameDialog('New Trigger', freshName('Untitled Trigger', mapData.triggers.map(t => t.name)), name => {
            // The new one is picked: selected once the tree comes.
            mapSelected = {kind: 'trigger', key: 0}
            mapApi.postMessage({type: 'newTrigger', category, name})
        })
    },
    newVariable() {
        mapApi.postMessage({type: 'types', line: 0})
    },
    rename() {
        if (!mapData) return
        const sel = mapSelected
        if (sel.kind === 'category') {
            const c = mapData.categories.find(c => c.index === sel.key)
            if (c) nameDialog('Rename Category', c.name, name => mapApi.postMessage({type: 'renameCategory', index: c.index, name}))
        } else if (sel.kind === 'trigger') {
            const input = /** @type {HTMLInputElement | null} */ ($('name'))
            input?.focus()
            input?.select()
        } else if (sel.kind === 'variable') {
            mapApi.postMessage({type: 'types', line: sel.key})
        }
    },
    delete() {
        if (!mapData) return
        const sel = mapSelected
        if (sel.kind === 'trigger') {
            const t = mapData.triggers[sel.key - 1]
            if (t) mapApi.postMessage({type: 'deleteTrigger', name: t.name})
        } else if (sel.kind === 'category') {
            const c = mapData.categories.find(c => c.index === sel.key)
            const count = mapData.triggers.filter(t => t.category === sel.key).length
            if (c) mapApi.postMessage({type: 'deleteCategory', index: c.index, name: c.name, count})
        } else if (sel.kind === 'variable') {
            mapApi.postMessage({type: 'deleteVariable', line: sel.key})
        }
    },
}

function updateMapTools() {
    const set = (/** @type {string} */ id, /** @type {boolean} */ on) => {
        const b = /** @type {HTMLButtonElement | null} */ ($(id))
        if (b) b.disabled = !on
    }
    const data = mapData
    const sel = mapSelected
    const there = !!data && (sel.kind === 'trigger' ? !!data.triggers[sel.key - 1]
        : sel.kind === 'category' ? data.categories.some(c => c.index === sel.key)
            : sel.kind === 'variable' ? data.variables.some(v => v.line === sel.key) : false)
    set('map-new-category', !!data)
    set('map-new-trigger', !!data)
    set('map-new-variable', !!data)
    set('map-rename', there)
    set('map-delete', there)
}

for (const [id, act] of /** @type {[string, () => void][]} */ ([
    ['map-new-category', mapActions.newCategory],
    ['map-new-trigger', mapActions.newTrigger],
    ['map-new-variable', mapActions.newVariable],
    ['map-rename', mapActions.rename],
    ['map-delete', mapActions.delete],
])) $(id)?.addEventListener('click', act)

mapTree.addEventListener('keydown', e => {
    if (e.key === 'Delete' || (e.key === 'Backspace' && (e.metaKey || e.ctrlKey))) {
        e.preventDefault()
        return mapActions.delete()
    }
    if (e.key === 'F2') {
        e.preventDefault()
        return mapActions.rename()
    }
    if (e.key === 'Enter') {
        e.preventDefault()
        if (mapSelected.kind === 'category' || mapSelected.kind === 'variables') toggle(mapSelected)
        else if (mapSelected.kind === 'variable') mapActions.rename()
        else if (mapSelected.kind === 'script') mapApi.postMessage({type: 'openCode', nth: 0})
        else if (mapSelected.kind === 'trigger' && mapData?.triggers[mapSelected.key - 1]?.custom) mapApi.postMessage({type: 'openCode', nth: mapSelected.key})
        return
    }
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
    if (e.altKey) {
        e.preventDefault()
        return stepTrigger(e.key === 'ArrowUp' ? -1 : 1)
    }
    const i = mapRows.findIndex(r => same(r.sel, mapSelected))
    const next = mapRows[Math.max(0, Math.min(mapRows.length - 1, i + (e.key === 'ArrowUp' ? -1 : 1)))]
    if (next) choose(next.sel)
    e.preventDefault()
})

/** Moves the selected trigger one up (`-1`) or down (`1`) in its category. @param {number} by */
function stepTrigger(by) {
    if (!mapData || mapSelected.kind !== 'trigger') return
    const data = mapData
    const nth = mapSelected.key
    const t = data.triggers[nth - 1]
    if (!t) return
    const siblings = data.triggers.map((x, k) => ({x, nth: k + 1})).filter(s => s.x.category === t.category)
    const i = siblings.findIndex(s => s.nth === nth)
    if (by < 0 && i > 0) moveTrigger(nth, t.category, siblings[i - 1].nth)
    else if (by > 0 && i < siblings.length - 1) moveTrigger(nth, t.category, siblings[i + 2] ? siblings[i + 2].nth : 0)
}

$('extract')?.addEventListener('click', () => mapApi.postMessage({type: 'extract'}))

const receive = (/** @type {any} */ msg) => {
    // The tests in the editor (`e2e/`): the line beside the tree dragged
    // `dx` and double clicked as a mouse does, what the tree's width was.
    if (msg.type === 'e2eSplitter') {
        const side = /** @type {HTMLElement | null} */ (root.querySelector('.map-side'))
        const splitter = $('map-splitter')
        if (!side || !splitter) return mapApi.postMessage({type: 'e2eSplitter', error: 'no tree beside the trigger'})
        const width = () => Math.round(side.getBoundingClientRect().width)
        const kept = () => (mapApi.getState() || {}).treeWidth ?? null
        const at = splitter.getBoundingClientRect()
        const x = at.left + at.width / 2, y = at.top + at.height / 2
        const pointer = (/** @type {string} */ type, /** @type {number} */ px) => splitter.dispatchEvent(new PointerEvent(type, {clientX: px, clientY: y, pointerId: 77, bubbles: true, cancelable: true, isPrimary: true}))
        const before = width()
        pointer('pointerdown', x)
        pointer('pointermove', x + msg.dx / 2)
        const midway = width()
        pointer('pointermove', x + msg.dx)
        pointer('pointerup', x + msg.dx)
        const dragged = width(), keptDragged = kept()
        splitter.dispatchEvent(new MouseEvent('dblclick', {clientX: x, clientY: y, bubbles: true}))
        mapApi.postMessage({type: 'e2eSplitter', before, midway, dragged, keptDragged, reset: width(), keptReset: kept(), max: Math.round(window.innerWidth * 0.6)})
        return
    }
    if (msg.type === 'tree') {
        mapData = msg.tree
        if (!mapData) return
        // The trigger's variable dialog edits these.
        setMapVariables(mapData.variables)
        // The picked trigger is the one selected.
        if (mapSelected.kind === 'trigger') mapSelected = {kind: 'trigger', key: mapData.picked}
        drawTree()
        // No trigger: nothing to show on the right but how to make one.
        const none = !mapData.triggers.length
        root.classList.toggle('empty', none)
        const loading = $('loading')
        if (loading) {
            if (none) loading.hidden = false
            loading.textContent = none ? 'The map has no triggers: New Trigger (above the tree) makes one.' : 'Reading the trigger…'
        }
    } else if (msg.type === 'icons') {
        mapIcons = msg.icons || {}
        drawTree()
    }
}

// The line between the tree and the trigger sizes the tree, as it's
// dragged; the width is kept with the page's state.
{
    const side = /** @type {HTMLElement} */ (root.querySelector('.map-side'))
    const splitter = $('map-splitter')
    const kept = (mapApi.getState() || {}).treeWidth
    if (side && kept) side.style.width = `${kept}px`
    splitter?.addEventListener('pointerdown', e => {
        if (!side) return
        e.preventDefault()
        // (A pointer of the tests' has nothing to capture.)
        try { splitter.setPointerCapture(e.pointerId) } catch { /* moves still reach it */ }
        splitter.classList.add('dragging')
        root.classList.add('resizing')
        const start = e.clientX
        const width = side.getBoundingClientRect().width
        const move = (/** @type {PointerEvent} */ m) => {
            const w = Math.max(140, Math.min(window.innerWidth * 0.6, width + m.clientX - start))
            side.style.width = `${w}px`
        }
        const up = () => {
            splitter.removeEventListener('pointermove', move)
            splitter.removeEventListener('pointerup', up)
            splitter.classList.remove('dragging')
            root.classList.remove('resizing')
            mapApi.setState({...(mapApi.getState() || {}), treeWidth: Math.round(side.getBoundingClientRect().width)})
        }
        splitter.addEventListener('pointermove', move)
        splitter.addEventListener('pointerup', up)
    })
    // Double click: back to its width at first.
    splitter?.addEventListener('dblclick', () => {
        if (!side) return
        side.style.width = ''
        mapApi.setState({...(mapApi.getState() || {}), treeWidth: undefined})
    })
}
return receive
}

// ---------------------------------------------------------------- the trigger
function mount(/** @type {HTMLElement} */ root, /** @type {any} */ host) {
const vscode = host

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (root.querySelector(`#${CSS.escape(id)}`))

/** @typedef {string | {r: string, p: string, s: Span[]}} Span */
/** @typedef {{label: string, group: number, kind: number, nodes: Node[]}} Block */
/** @typedef {{line: number, kind: number, enabled: boolean, name: string, known: boolean, icon: string, hint: string, text: Span[], blocks: Block[]}} Node */
/** @typedef {{line: number, scope: string, name: string, type: string, type_name: string, array: boolean, size: number, initial: string | null}} Variable */
/**
 * @typedef {{
 *   trigger: boolean, script: string, variables: Variable[],
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
/** The selected row's path: where the keys and the mouse are. */
let selected = saved.selected || ''
/** Where a Shift-selection started: with `selected`, its two ends. */
let anchor = selected
/**
 * The paths of the selected lines: one with a click, from the anchor with
 * Shift, one more or less with Ctrl (Cmd on a Mac). Empty: the selected
 * row alone.
 * @type {Set<string>}
 */
let chosen = new Set()
/** A line to select once a view has it (after an operation). */
let selectLine = 0
/**
 * A map's triggers (`TriggerEditorMap` sets it): its variables, edited from its tree
 * by their lines in the text; globals only, World Editor's.
 * @type {Variable[] | null}
 */
let mapVariables = null

const tree = $('tree')
const save = () => vscode.setState({...(vscode.getState() || {}), collapsed: [...collapsed], selected})

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
        if (r) anchor = selected = r.path
        selectLine = 0
    }
    if (!rows.some(r => r.path === selected)) selected = rows.length ? rows[0].path : ''
    if (!rows.some(r => r.path === anchor)) anchor = selected
    chosen = new Set([...chosen].filter(p => rows.some(r => r.path === p)))
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
    if (row.path === selected) el.classList.add('focused')
    if (row.path === selected && !chosen.size || chosen.has(row.path)) el.classList.add('selected')
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

/**
 * The lines from `anchor` to `path`, beside each other in the anchor's
 * heading or block (a far end in another block: as far as it goes).
 * @param {string} path @returns {Row[]}
 */
function range(path) {
    const start = rows.find(r => r.path === anchor)
    const end = rows.find(r => r.path === path)
    if (!end) return []
    if (!start || start === end || start.type !== 'node' || end.type !== 'node') return [end]
    let [a, b] = [rows.indexOf(start), rows.indexOf(end)]
    if (a > b) [a, b] = [b, a]
    return rows.slice(a, b + 1).filter(r => r.type === 'node' && r.parent === start.parent && r.group === start.group && r.kind === start.kind)
}

/**
 * The selected rows, in the tree's order: the chosen lines (one inside
 * another counts with it), else the selected row.
 * @returns {Row[]}
 */
function selection() {
    if (!chosen.size) {
        const row = selectedRow()
        return row ? [row] : []
    }
    const picked = rows.filter(r => chosen.has(r.path))
    return picked.filter(r => !picked.some(o => o !== r && r.path.startsWith(`${o.path}/`)))
}

/**
 * Selects `path`: alone; with `how` 'range', the lines from the anchor to
 * it; with 'toggle', one line more or less.
 * @param {string} path @param {'one' | 'range' | 'toggle' | boolean} [how]
 */
function select(path, how = 'one') {
    if (how === true) how = 'range'
    if (how === false) how = 'one'
    const row = rows.find(r => r.path === path)
    const line = !!row && row.type === 'node'
    if (how === 'toggle' && line) {
        // The one selected before is a line chosen too.
        const before = selectedRow()
        if (!chosen.size && before && before.type === 'node') chosen.add(before.path)
        if (chosen.has(path)) chosen.delete(path)
        else chosen.add(path)
        anchor = path
    } else if (how === 'range' && line) {
        chosen = new Set(range(path).map(r => r.path))
    } else {
        chosen = new Set()
        anchor = path
    }
    selected = path
    for (const el of tree.querySelectorAll('.row')) {
        const p = /** @type {HTMLElement} */ (el).dataset.path || ''
        el.classList.toggle('selected', chosen.size ? chosen.has(p) : p === path)
        el.classList.toggle('focused', p === path)
    }
    const el = tree.querySelector(`.row[data-path="${CSS.escape(path)}"]`)
    if (el) el.scrollIntoView({block: 'nearest'})
    updateTools()
    save()
}

/**
 * Where the selected lines move one step up or down, when they're all in
 * one heading or block: a place, else null.
 * @param {'up' | 'down'} what
 */
function stepPlace(what) {
    const all = selection()
    const first = all[0]
    if (!first || all.some(r => r.type !== 'node' || r.parent !== first.parent || r.group !== first.group || r.kind !== first.kind)) return null
    const low = Math.min(...all.map(r => r.index))
    const high = Math.max(...all.map(r => r.index))
    if (what === 'up') return low > 0 ? `${first.parent} ${first.group} ${low - 1}` : null
    return high < first.count - 1 ? `${first.parent} ${first.group} ${high + 2}` : null
}

/** The lines of the selected lines: the first, and the others. */
function selectedLines() {
    const lines = selection().filter(r => r.node).map(r => /** @type {Node} */ (r.node).line)
    return {line: lines[0], lines: lines.slice(1).join(' ')}
}

// ---------------------------------------------------------------- operations

/** Where a paste goes for the selected row: after a line, at a block's or heading's end. */
function pastePlace() {
    const all = selection()
    const row = all.length > 1 ? all[all.length - 1] : selectedRow()
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
    set('up', !!node && !!stepPlace('up'))
    set('down', !!node && !!stepPlace('down'))
    set('paste', !!view)
    set('new', !!view && !!row && row.kind >= 0)
    set('new-variable', !!view)
}

/** @param {'toggle' | 'delete' | 'copy' | 'cut' | 'up' | 'down' | 'paste'} what */
function act(what) {
    const row = selectedRow()
    if (what === 'paste') return vscode.postMessage({type: 'paste', place: pastePlace()})
    if (what === 'delete' && row && row.type === 'variable' && row.variable) {
        return vscode.postMessage({type: 'deleteVariable', line: row.variable.line})
    }
    if (!row || row.type !== 'node' || !row.node) return
    const line = row.node.line
    if (what === 'toggle' || what === 'delete' || what === 'copy' || what === 'cut') {
        return vscode.postMessage({type: what, ...selectedLines()})
    }
    if (what === 'up' || what === 'down') {
        const place = stepPlace(what)
        if (place) vscode.postMessage({type: 'move', ...selectedLines(), place})
        return
    }
    vscode.postMessage({type: what, line})
}

$('new').addEventListener('click', () => newLine())
$('new-variable').addEventListener('click', () => vscode.postMessage({type: 'types'}))
$('convert')?.addEventListener('click', () => vscode.postMessage({type: 'convert'}))

for (const id of ['toggle', 'delete', 'copy', 'cut', 'up', 'down', 'paste']) {
    $(id).addEventListener('click', () => {
        act(/** @type {any} */ (id))
        tree.focus()
    })
}

// ---------------------------------------------------------------- mouse and keys

/** Pressed on a line of several selected: one alone on the click, unless it's dragged. */
let pressed = ''
const isMac = navigator.platform.toUpperCase().includes('MAC')
tree.addEventListener('mousedown', e => {
    pressed = ''
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    if (!el || !el.dataset.path || e.button !== 0) return
    const path = el.dataset.path
    if (e.shiftKey) select(path, 'range')
    else if (isMac ? e.metaKey : e.ctrlKey) select(path, 'toggle')
    else if (chosen.size > 1 && chosen.has(path)) pressed = path
    else select(path)
})

/** The row under a mouse event, and the parameter link, if it's on one. */
function hit(/** @type {MouseEvent} */ e) {
    const target = /** @type {HTMLElement} */ (e.target)
    if (target.closest('.twisty')) return null
    const el = /** @type {HTMLElement | null} */ (target.closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    if (!row) return null
    const link = /** @type {HTMLElement | null} */ (target.closest('.link'))
    return {row, link: link && row.node && link.dataset.path ? link : null}
}

// A click on a parameter opens its dialog at once; elsewhere on a row it
// only selects, and a double click opens the row's own (a line's
// function, a variable's name, type ...) or folds a heading or block.
tree.addEventListener('click', e => {
    const at = hit(e)
    if (pressed && at && at.row.path === pressed) select(pressed)
    pressed = ''
    // With Shift or Ctrl it's selecting, not editing.
    if (e.shiftKey || e.ctrlKey || e.metaKey) return
    if (at && at.link && at.row.node) vscode.postMessage({type: 'choices', line: at.row.node.line, path: at.link.dataset.path})
})

tree.addEventListener('dblclick', e => {
    if (e.shiftKey || e.ctrlKey || e.metaKey) return
    const at = hit(e)
    if (at && !at.link) editRow(at.row)
})

/** @param {Row} row */
function editRow(row) {
    if (row.type === 'node' && row.node) vscode.postMessage({type: 'catalog', kind: row.node.kind, line: row.node.line})
    else if (row.type === 'variable' && row.variable) vscode.postMessage({type: 'types', line: row.variable.line})
    else if (row.hasChildren) setOpen(row, !row.open)
}

tree.addEventListener('keydown', e => {
    const row = selectedRow()
    const i = row ? rows.indexOf(row) : -1
    const mod = e.ctrlKey || e.metaKey
    const key = e.key
    let handled = true
    if (e.altKey && key === 'ArrowUp') act('up')
    else if (e.altKey && key === 'ArrowDown') act('down')
    else if (key === 'ArrowUp') { if (i > 0) select(rows[i - 1].path, e.shiftKey) }
    else if (key === 'ArrowDown') { if (i < rows.length - 1) select(rows[i + 1].path, e.shiftKey) }
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
    } else if (key === 'Enter' && row) editRow(row)
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

/** The lines dragged: the selected ones, when it's one of them. @type {Row[]} */
let dragged = []
/** @type {{row: Row, place: string, cls: string} | null} */
let dropAt = null

const clearDrop = () => {
    for (const el of tree.querySelectorAll('.drop-before, .drop-after, .drop-into')) el.classList.remove('drop-before', 'drop-after', 'drop-into')
}

tree.addEventListener('dragstart', e => {
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    if (!el || !row || row.type !== 'node' || !e.dataTransfer) return
    pressed = ''
    if (!selection().includes(row)) select(row.path)
    dragged = selection().filter(r => r.type === 'node')
    e.dataTransfer.effectAllowed = 'move'
    e.dataTransfer.setData('text/plain', dragged.map(r => r.path).join('\n'))
    for (const r of dragged) tree.querySelector(`.row[data-path="${CSS.escape(r.path)}"]`)?.classList.add('dragging')
})

tree.addEventListener('dragend', () => {
    for (const el of tree.querySelectorAll('.dragging')) el.classList.remove('dragging')
    clearDrop()
    dragged = []
    dropAt = null
})

tree.addEventListener('dragover', e => {
    if (!dragged.length || !e.dataTransfer) return
    const el = /** @type {HTMLElement | null} */ (/** @type {HTMLElement} */ (e.target).closest('.row'))
    const row = el && rows.find(r => r.path === el.dataset.path)
    clearDrop()
    dropAt = null
    if (!el || !row) return
    // Not into itself.
    if (dragged.some(d => row.path === d.path || row.path.startsWith(`${d.path}/`))) return
    if (row.group < 0) return
    if (dragged.some(d => d.kind !== row.kind)) return
    if (row.type === 'node') {
        const box = el.getBoundingClientRect()
        const after = e.clientY > box.top + box.height / 2
        dropAt = {row, place: `${row.parent} ${row.group} ${row.index + (after ? 1 : 0)}`, cls: after ? 'drop-after' : 'drop-before'}
    } else {
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
    const lines = dragged.filter(r => r.node).map(r => /** @type {Node} */ (r.node).line)
    if (lines.length && dropAt) vscode.postMessage({type: 'move', line: lines[0], lines: lines.slice(1).join(' '), place: dropAt.place})
    dragged = []
    dropAt = null
})

// ---------------------------------------------------------------- fields

const nameInput = /** @type {HTMLInputElement} */ ($('name'))
const description = /** @type {HTMLTextAreaElement} */ ($('description'))
const enabled = /** @type {HTMLInputElement} */ ($('enabled'))
const initiallyOn = /** @type {HTMLInputElement} */ ($('initially-on'))

const field = (/** @type {string} */ key, /** @type {string} */ value) => vscode.postMessage({type: 'field', key, value})

/**
 * A text field written into the document as it's typed (a moment after the
 * last key), so the editor shows it changed at once and saving keeps it;
 * Escape puts back what it was when it got the focus. The name, once it's
 * done (Enter, or the focus gone) and other than it was, has the
 * references to the trigger follow it (`renamed`, from what it was).
 * @param {HTMLInputElement | HTMLTextAreaElement} input @param {'name' | 'description'} key
 */
function liveField(input, key) {
    let timer = 0
    let sent = ''
    let before = ''
    const send = () => {
        clearTimeout(timer)
        if (view && input.value !== sent) {
            sent = input.value
            field(key, input.value)
        }
    }
    input.addEventListener('focus', () => { before = sent = input.value })
    input.addEventListener('input', () => {
        clearTimeout(timer)
        timer = setTimeout(send, 300)
    })
    input.addEventListener('blur', () => {
        send()
        if (key === 'name' && view && input.value !== before) vscode.postMessage({type: 'renamed', from: before})
        before = input.value
    })
    input.addEventListener('keydown', e => {
        const k = /** @type {KeyboardEvent} */ (e)
        if (k.key === 'Enter' && input === nameInput) {
            k.preventDefault()
            tree.focus()
        } else if (k.key === 'Escape') {
            input.value = before
            send()
            tree.focus()
        }
    })
}
liveField(nameInput, 'name')
liveField(description, 'description')
enabled.addEventListener('change', () => field('enabled', enabled.checked ? '1' : '0'))
initiallyOn.addEventListener('change', () => field('initially_off', initiallyOn.checked ? '0' : '1'))

$('problems').addEventListener('click', () => {
    if (view && view.errors.length) vscode.postMessage({type: 'reveal', line: view.errors[0][0]})
})
$('open-text').addEventListener('click', () => vscode.postMessage({type: 'openText'}))
for (const language of ['jass', 'lua', 'angelscript']) {
    $(`show-${language}`)?.addEventListener('click', () => vscode.postMessage({type: 'show', language}))
}

/** @param {View} v */
function showFields(v) {
    // A comment is text between triggers, not a trigger: its text alone,
    // set apart (no events, conditions or actions, no switches).
    const comment = !!v.comment
    root.classList.toggle('comment-trigger', comment)
    // Custom text: the trigger is its code, which is the custom script's
    // (war3map.wct) and opens in an editor of its own; no events,
    // conditions or actions.
    const custom = !comment && !!v.custom_text
    root.classList.toggle('custom-trigger', custom)
    description.placeholder = comment ? 'The comment' : 'Comment'
    const art = icons[v.enabled !== 0 ? 'SEIcon_Trigger' : 'SEIcon_TriggerDisabled']
    $('title-icon').innerHTML = comment ? '<i class="fa-solid fa-comment"></i>'
        : art ? `<img class="icon art" alt="" src="${art}">` : '<i class="fa-solid fa-bolt"></i>'

    // What's being typed stays.
    if (document.activeElement !== nameInput) nameInput.value = v.name
    nameInput.title = `The trigger's name; in the map's script ${v.script}, as World Editor names it`
    if (document.activeElement !== description) description.value = v.description
    enabled.checked = v.enabled !== 0
    initiallyOn.checked = v.initially_off === 0
    const problems = $('problems')
    problems.hidden = !v.errors.length
    problems.textContent = `${v.errors.length} problem${v.errors.length === 1 ? '' : 's'}`
    problems.title = v.errors.slice(0, 10).map(([line, m]) => `Line ${line}: ${m}`).join('\n')
    const notes = []
    if (!v.data) notes.push('No UI\\TriggerData.txt in the project\'s game folders or map roots: functions show as calls, without their phrases.')
    if (v.comment) notes.push('A comment (comment=1): text between triggers, nothing of it is in the map\'s script.')
    const notice = $('notice')
    notice.hidden = !notes.length && !custom
    notice.textContent = notes.join(' ')
    if (custom) {
        // A map's: its code opens beside; a trigger file's is in the map's custom script.
        const wtg = root.classList.contains('wtg')
        const text = wtg
            ? 'Custom text: this trigger is its code, which is the map\'s custom script\'s (war3map.wct) and opens in an editor of its own, beside. '
            : 'Custom text: this trigger is its code, in the .j file of its name beside it. '
        notice.append(notes.length ? ' ' : '', text)
        const open = document.createElement('button')
        open.className = 'text-button'
        open.textContent = 'Open Code'
        open.addEventListener('click', () => vscode.postMessage({type: 'openCode', nth: 0, picked: true}))
        notice.append(open)
    }
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
 * unless it returns false. With a `line`, a button opens the text there
 * (`textLine`: a line of the whole text, not of the trigger's).
 * @param {string} title @param {HTMLElement[]} body @param {() => boolean | void} ok @param {number} [line] @param {boolean} [textLine]
 */
function openDialog(title, body, ok, line = 0, textLine = false) {
    const reveal = line ? [h('button', {type: 'button', class: 'text-button', title: 'Open the text at this line', onclick: () => {
        closeDialog()
        vscode.postMessage({type: textLine ? 'revealText' : 'reveal', line})
    }}, ['Show in Text']), h('div', {class: 'spacer'})] : []
    box.replaceChildren(
        h('div', {class: 'dialog-title'}, [title]),
        ...body,
        h('div', {class: 'dialog-buttons'}, [
            ...reveal,
            h('button', {type: 'button', class: 'text-button', onclick: closeDialog}, ['Cancel']),
            h('button', {type: 'submit', class: 'primary-button'}, ['OK']),
        ]),
    )
    box.onsubmit = e => {
        e.preventDefault()
        if (ok() !== false) closeDialog()
    }
    openedAt = performance.now()
    overlay.hidden = false
}

overlay.addEventListener('keydown', e => {
    if (e.key === 'Escape') {
        e.preventDefault()
        closeDialog()
    }
})
/** When the dialog opened: the second click of a double click that
 * opened it lands on the overlay, and mustn't close it. */
let openedAt = 0
overlay.addEventListener('mousedown', e => {
    if (e.target === overlay && performance.now() - openedAt > 500) closeDialog()
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
 *   variables: [string, string, boolean, string][], calls: [string, string, string][], value: boolean,
 *   objects?: [string, string][]}} c
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
    if (c.value && c.objects && c.objects.length) {
        // Objects (units, abilities ...): by name, the id beside it; an id
        // the list doesn't have is typed into the filter.
        const current = c.current.kind === 'value' ? c.current.value : ''
        const {filter, list} = pickList(c.objects.map(([id, name]) => [id, `${name} [${id}]`]), current, submit)
        option('value', 'Value', [filter, list], () => {
            if (list.value) return ['value', list.value]
            const typed = filter.value.trim()
            return typed ? ['value', typed] : null
        })
    } else if (c.value) {
        // Custom script: lines, each a line of JASS (Ctrl+Enter is OK); in
        // a map's triggers one line, as World Editor's.
        const lines = c.type === 'scriptcode' && mapVariables === null
        const input = /** @type {HTMLInputElement} */ (lines
            ? h('textarea', {class: 'wide code', rows: 8, spellcheck: false, wrap: 'off'})
            : h('input', {type: 'text', class: 'wide', spellcheck: false}))
        if (lines) {
            input.addEventListener('keydown', e => {
                const k = /** @type {KeyboardEvent} */ (e)
                if (k.key === 'Enter' && (k.ctrlKey || k.metaKey)) {
                    k.preventDefault()
                    submit()
                } else if (k.key === 'Tab' && !k.shiftKey) {
                    // A tab indents, as in an editor.
                    k.preventDefault()
                    input.setRangeText('    ', input.selectionStart ?? 0, input.selectionEnd ?? 0, 'end')
                }
            })
        }
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

/** The line at `line` in the view. @param {number} line @returns {Node | null} */
function findNode(line) {
    /** @param {Node[]} nodes @returns {Node | null} */
    const find = nodes => {
        for (const n of nodes) {
            if (n.line === line) return n
            for (const b of n.blocks) {
                const got = find(b.nodes)
                if (got) return got
            }
        }
        return null
    }
    return view ? find([...view.events, ...view.conditions, ...view.actions]) : null
}

/** New: asks the server for the functions of the selected row's kind. */
function newLine() {
    const row = selectedRow()
    if (!view || !row || row.kind < 0) return
    vscode.postMessage({type: 'catalog', kind: row.kind, place: pastePlace()})
}

/**
 * The dialog of a new line: a category, then a function in it. With a
 * `line`, of the line there: what's picked replaces its function.
 * @param {number} kind @param {string} place @param {[string, string, string, string, boolean][]} items @param {number} [line]
 */
function newDialog(kind, place, items, line = 0) {
    const node = line ? findNode(line) : null
    const categories = [...new Set(items.map(i => i[2]))]
    const category = /** @type {HTMLSelectElement} */ (h('select', {class: 'wide'},
        [h('option', {value: '\u0000'}, ['- All -']), ...categories.map(cat => h('option', {value: cat}, [cat || '(none)']))]))
    const hint = h('div', {class: 'hint'})
    let picker = pickList([], node ? node.name : '', () => box.requestSubmit())
    const holder = h('div', {class: 'picker'})
    const fill = () => {
        const all = category.value === '\u0000'
        const shown = items.filter(i => all || i[2] === category.value)
        const q = picker.filter.value
        picker = pickList(shown.map(([n, t, cat, , show]) => [n, all && cat && show ? `${cat} - ${t}` : t]), node ? node.name : '', () => box.requestSubmit())
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
    const current = node && items.find(i => i[0] === node.name)
    if (current) {
        category.value = current[2]
        fill()
    }
    const title = (line ? ['Edit Event', 'Edit Condition', 'Edit Action'] : ['New Event', 'New Condition', 'New Action'])[kind] || 'New'
    const body = [h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, ['Category']), category]), holder, hint]
    if (node && node.blocks.length) body.push(h('div', {class: 'hint'}, ["Another function keeps what's inside only in the blocks it has."]))
    openDialog(title, body, () => {
        if (!picker.list.value) return false
        if (!line) vscode.postMessage({type: 'new', place, kind, name: picker.list.value})
        else if (!node || picker.list.value !== node.name) vscode.postMessage({type: 'replace', line, name: picker.list.value})
    }, line)
    if (line) picker.list.focus()
    else picker.filter.focus()
}

/**
 * The dialog of a new variable: name, type, scope, array and size,
 * initial value (no scope in a map's triggers: their variables are
 * globals). With a `line`, of the variable there: OK rewrites it.
 * @param {[string, string][]} types @param {number} [line]
 */
function variableDialog(types, line = 0) {
    const list = mapVariables ?? (view ? view.variables : [])
    const v = line ? list.find(x => x.line === line) : null
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
    if (v) {
        name.value = v.name
        if (![...type.options].some(o => o.value === v.type)) type.append(h('option', {value: v.type}, [v.type_name || v.type]))
        type.value = v.type
        scope.value = v.scope || 'global'
        array.checked = v.array
        size.disabled = !v.array
        size.value = String(v.size || 1)
        initial.value = v.initial ?? ''
    }
    const row = (/** @type {string} */ label, /** @type {HTMLElement[]} */ controls) =>
        h('div', {class: 'choice'}, [h('label', {class: 'choice-label'}, [label]), ...controls])
    openDialog(v ? 'Edit Variable' : 'New Variable', [
        row('Name', [name]),
        row('Type', [type]),
        // A map's are globals.
        ...(mapVariables ? [] : [row('Scope', [scope])]),
        h('label', {class: 'check'}, [array, 'Array, of size ', size]),
        row('Initial value', [initial]),
    ], () => {
        if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(name.value.trim())) {
            name.focus()
            return false
        }
        vscode.postMessage({
            type: v ? 'setVariable' : 'newVariable', line, name: name.value.trim(), vtype: type.value, array: array.checked,
            size: size.value.trim() || '1', scope: mapVariables ? 'global' : scope.value, initial: initial.value,
        })
    }, line, !!mapVariables)
    name.focus()
    if (v) name.select()
}

// ---------------------------------------------------------------- messages

// What the extension posts: this, and a map's tree's (`wtg`).
const receiveMap = root.classList.contains('wtg') ? TriggerEditorMap({vscode, h, openDialog, root, $, setMapVariables: v => (mapVariables = v)}) : null
const receive = (/** @type {any} */ msg) => {
    receiveMap?.(msg)
    if (msg.type === 'view') {
        // No name: the file's (a build names it so).
        if (msg.file) nameInput.placeholder = msg.file
        // The text changed: the lines chosen are other lines now.
        if (view) chosen = new Set()
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
        variableDialog(msg.items, msg.line)
    } else if (msg.type === 'catalog') {
        newDialog(msg.kind, msg.place, msg.items, msg.line)
    } else if (msg.type === 'select') {
        const r = rows.find(r => r.type === 'node' && r.node && r.node.line === msg.line)
        if (r) select(r.path)
    }
}

vscode.postMessage({type: 'ready'})
tree.focus()
return {receive}
}
return {mount}
})()
