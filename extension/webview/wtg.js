// @ts-check
'use strict'

/**
 * The tree of a map's triggers (`war3map.wtg`), beside the trigger picked
 * in it (`trg.js` shows that one; `extension.js`, `registerTrigger`).
 * There are no files to show them: the server reads the tree from the
 * text (`triggers::wtg_view`) and this page draws it, as World Editor
 * does: each category with its triggers (those of a category the file
 * hasn't at the top), then the variables.
 *
 * A click selects a row; a trigger selected is the one picked, shown on
 * the right. The tools above the tree (and the keys) make a new category,
 * trigger or variable, rename (F2) or delete (Delete) what's selected:
 * a category's name in a dialog, a trigger's in its name field, a
 * variable in its dialog (`trg.js`'s, without a scope: a map's variables
 * are globals). A double click on a category opens or closes it, on a
 * variable edits it. A category collapses by its index, kept while the
 * text changes. A trigger is moved by dragging it: onto a trigger, before
 * it (in its category); onto a category, to its end; Alt+Up and Alt+Down
 * move the selected one in its category.
 *
 * Loaded after `trg.js`, in the same global scope: its `vscode` (the API
 * can be acquired once), `h`, `openDialog` and `mapVariables` are used
 * here, so nothing here is named as anything there.
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

// @ts-ignore: trg.js's
const mapApi = vscode

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

const mapTree = /** @type {HTMLElement} */ (document.getElementById('map-tree'))
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
    // @ts-ignore: trg.js's
    const input = /** @type {HTMLInputElement} */ (h('input', {type: 'text', class: 'wide', spellcheck: false, value}))
    // @ts-ignore: trg.js's
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
            const input = /** @type {HTMLInputElement | null} */ (document.getElementById('name'))
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
        const b = /** @type {HTMLButtonElement | null} */ (document.getElementById(id))
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
])) document.getElementById(id)?.addEventListener('click', act)

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

document.getElementById('extract')?.addEventListener('click', () => mapApi.postMessage({type: 'extract'}))

window.addEventListener('message', e => {
    const msg = e.data
    if (msg.type === 'tree') {
        mapData = msg.tree
        if (!mapData) return
        // trg.js's variable dialog edits these.
        // @ts-ignore: trg.js's
        mapVariables = mapData.variables
        // The picked trigger is the one selected.
        if (mapSelected.kind === 'trigger') mapSelected = {kind: 'trigger', key: mapData.picked}
        drawTree()
        // No trigger: nothing to show on the right but how to make one.
        const none = !mapData.triggers.length
        document.body.classList.toggle('empty', none)
        const loading = document.getElementById('loading')
        if (loading) {
            if (none) loading.hidden = false
            loading.textContent = none ? 'The map has no triggers: New Trigger (above the tree) makes one.' : 'Reading the trigger…'
        }
    } else if (msg.type === 'icons') {
        mapIcons = msg.icons || {}
        drawTree()
    }
})

// The line between the tree and the trigger sizes the tree, as it's
// dragged; the width is kept with the page's state.
{
    const side = /** @type {HTMLElement} */ (document.querySelector('.map-side'))
    const splitter = document.getElementById('map-splitter')
    const kept = (mapApi.getState() || {}).treeWidth
    if (side && kept) side.style.width = `${kept}px`
    splitter?.addEventListener('pointerdown', e => {
        if (!side) return
        e.preventDefault()
        splitter.setPointerCapture(e.pointerId)
        splitter.classList.add('dragging')
        document.body.classList.add('resizing')
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
            document.body.classList.remove('resizing')
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
