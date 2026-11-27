'use strict'

/**
 * The object editor, a panel over the view: a kind's objects (the game's,
 * changed by the map or not, and the map's own), searched; the one picked's
 * fields, each its value (the map's change, else the game's) set as it's
 * typed, empty for the game's again; New (a custom object based on the one
 * picked), Delete (a custom one; a standard one's changes). The editor's
 * process does each (`editor/src/objdata.rs`) and sends the list again;
 * this only shows and asks.
 */
const EditorObjectEditor = (() => {
    const KINDS = ['Units', 'Items', 'Destructables', 'Doodads', 'Abilities', 'Buffs', 'Upgrades']
    /** World Editor's names of the metadata's categories. */
    const CATEGORIES = {
        abil: 'Abilities', art: 'Art', combat: 'Combat', data: 'Data', editor: 'Editor', move: 'Movement', path: 'Pathing',
        sound: 'Sound', stats: 'Stats', tech: 'Techtree', text: 'Text',
    }
    const STATES = ['', 'changed', 'custom']
    /** The kinds (`KINDS`' indices) a field type names objects of (`objdata::id_kinds`). */
    const ID_KINDS = {
        abilityList: [4], heroAbilityList: [4], abilCode: [4], abilityCode: [4],
        unitList: [0], unitCode: [0], itemList: [1], itemCode: [1], upgradeList: [6], upgradeCode: [6],
        buffList: [5], buffCode: [5], effectList: [5], techList: [0, 6, 1], destructableList: [2], destructableCode: [2],
    }

    /** An icon (`ImageData`) drawn on a canvas, its size. */
    const paint = (canvas, image) => {
        if (!image) return
        canvas.width = image.width
        canvas.height = image.height
        canvas.getContext('2d').putImageData(image, 0, 0)
        canvas.dataset.painted = '1'
    }

    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }

    class Panel {
        /**
         * @param root the panel's element
         * @param send sends a frame
         * @param encode the protocol's encoders
         * @param said what the panel shows, for the tests (`vscode.postMessage`)
         */
        constructor(root, send, encode, said) {
            this.root = root
            this.send = send
            this.encode = encode
            this.said = said
            this.kind = 0
            this.objects = []
            this.picked = null
            /** The picked one's fields as last sent. */
            this.fields = null
            this.filter = ''
            /** The field types' values (`ObjectEnums`), by type. */
            this.enums = new Map()
            /** Each kind's objects as last listed, by kind. */
            this.lists = new Map()
            /** The object copied: {kind, id}. */
            this.copied = null
            /** The id picker open: {kinds, update}. */
            this.picker = null
            /** Icons by path (`null`: none), and the canvases waiting for them. */
            this.icons = new Map()
            /** World Editor's View > Display Values as Raw Data (Ctrl+D): ids, not names; every value as it's stored. */
            this.raw = false
            this.build()
        }

        build() {
            this.tabs = el('div', {className: 'obj-tabs'})
            KINDS.forEach((name, k) => {
                this.tabs.append(el('button', {textContent: name, onclick: () => this.showKind(k)}))
            })
            this.search = el('input', {type: 'search', placeholder: 'Search', className: 'obj-search'})
            this.search.oninput = () => {
                this.filter = this.search.value
                this.renderList()
            }
            this.list = el('div', {className: 'obj-list'})
            this.newButton = el('button', {textContent: 'New', title: 'A custom object based on the one picked', onclick: () => {
                if (!this.picked) return
                // The new one comes picked (its fields with the list): the
                // one not among these (an answer on its way is another's).
                this.pendingNew = new Set(this.objects.map(o => o.id))
                this.send(this.encode.objectNew(this.kind, this.base()))
            }})
            this.deleteButton = el('button', {textContent: 'Delete', title: 'A custom object taken away; a standard one\'s changes taken out', onclick: () => this.picked && this.send(this.encode.objectDelete(this.kind, this.picked))})
            this.copyButton = el('button', {textContent: 'Copy', title: 'The one picked, to paste as a new one', onclick: () => {
                if (!this.picked) return
                this.copied = {kind: this.kind, id: this.picked}
                this.renderList()
            }})
            this.pasteButton = el('button', {textContent: 'Paste', title: 'A new one, a copy of the one copied (its base, its changes)', onclick: () => {
                if (this.copied?.kind !== this.kind) return
                // The one that comes not among these.
                this.pendingNew = new Set(this.objects.map(o => o.id))
                this.send(this.encode.objectNew(this.kind, this.copied.id))
            }})
            this.rawBox = el('input', {type: 'checkbox', title: 'Display values as raw data (Ctrl+D)'})
            this.rawBox.onchange = () => this.setRaw(this.rawBox.checked)
            // World Editor's File > Export and Import: the kind's file, by VS Code's dialogs.
            this.exportButton = el('button', {textContent: 'Export…', title: "This kind's object data written to a file (.w3u …), the map's strings as their text", onclick: () => this.said({dialog: 'export', kind: this.kind})})
            this.importButton = el('button', {textContent: 'Import…', title: "A file's object data (.w3u …) in place of this kind's", onclick: () => this.said({dialog: 'import', kind: this.kind})})
            this.title = el('div', {className: 'obj-title'})
            this.table = el('div', {className: 'obj-fields'})
            // Ctrl+D: raw data on and off, as in World Editor.
            this.root.addEventListener('keydown', e => {
                if ((e.ctrlKey || e.metaKey) && e.code === 'KeyD') {
                    e.preventDefault()
                    this.setRaw(!this.raw)
                }
            })
            this.root.tabIndex = -1
            this.root.append(
                el('div', {className: 'obj-side'}, this.tabs, this.search, this.list, el('div', {className: 'obj-buttons'}, this.newButton, this.copyButton, this.pasteButton, this.deleteButton, this.exportButton, this.importButton, el('label', {className: 'obj-raw'}, this.rawBox, 'Raw'))),
                el('div', {className: 'obj-main'}, this.title, this.table),
            )
        }

        open() {
            this.root.hidden = false
            this.showKind(this.kind)
        }

        close() {
            this.closePicker()
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
        }

        /** Raw data (ids, values as stored) on or off. */
        setRaw(on) {
            this.raw = on
            this.rawBox.checked = on
            this.renderList()
            this.renderFields()
        }

        /** The picked one's base: its own id for one of the game's. */
        base() {
            return this.fields?.id === this.picked ? this.fields.base : this.picked
        }

        showKind(k) {
            if (k !== this.kind) {
                this.kind = k
                this.picked = null
                this.fields = null
            }
            for (const [i, b] of [...this.tabs.children].entries()) b.classList.toggle('on', i === k)
            this.send(this.encode.objectList(k))
        }

        pick(id) {
            this.picked = id
            this.renderList()
            this.send(this.encode.objectAsk(this.kind, id))
        }

        /** A kind's objects (`objectList`): shown; the picked one's fields asked again. */
        setList(msg) {
            this.lists.set(msg.objectKind, msg.objects)
            this.picker?.update()
            if (msg.objectKind !== this.kind) return
            this.objects = msg.objects
            if (this.picked && !this.objects.some(o => o.id === this.picked)) {
                this.picked = null
                this.fields = null
            }
            this.renderList()
            if (this.picked) this.send(this.encode.objectAsk(this.kind, this.picked))
            else this.renderFields()
        }

        renderList() {
            const words = this.filter.toLowerCase().split(/\s+/).filter(Boolean)
            const shown = this.objects.filter(o => words.every(w => `${o.name} ${o.id} ${o.group}`.toLowerCase().includes(w)))
            // Under their places in World Editor's tree (`Melee · Human · Units`).
            const rows = []
            let group = null
            for (const o of shown) {
                if (o.group !== group) {
                    group = o.group
                    rows.push(el('div', {className: 'obj-group', textContent: group || 'Other'}))
                }
                const row = el('div', {className: `obj-row ${STATES[o.state]}${o.id === this.picked ? ' on' : ''}`, textContent: this.raw ? `${o.id} (${o.name})` : `${o.name} (${o.id})`, title: o.group})
                row.onclick = () => this.pick(o.id)
                rows.push(row)
            }
            this.list.replaceChildren(...rows)
            this.newButton.disabled = this.deleteButton.disabled = this.copyButton.disabled = !this.picked
            this.pasteButton.disabled = this.copied?.kind !== this.kind
            this.pasteButton.title = this.copied ? `A new one, a copy of ${this.copied.id}` : 'Copy one first'
            this.report()
        }

        /** The field types' values (`objectEnums`). */
        setEnums(list) {
            this.enums = new Map(list.map(e => [e.ty, e]))
            if (this.fields) this.renderFields()
        }

        /** An icon read (`icons`, kind 255): drawn where it's shown. */
        setIcon(path, image) {
            this.icons.set(path.toLowerCase(), image)
            for (const c of this.table.querySelectorAll('canvas.obj-icon')) if (c.dataset.path === path.toLowerCase()) paint(c, image)
            this.report()
        }

        /**
         * The control of a field: what it is by its type (a checkbox, a
         * choice, a list's checkboxes, text), sending `value` as text.
         */
        control(r, send) {
            // Raw data: every value as text, as it's stored.
            const values = this.raw ? undefined : this.enums.get(r.ty)
            if (r.ty === 'bool' && !this.raw) {
                const box = el('input', {type: 'checkbox', checked: r.value.trim() === '1'})
                box.onchange = () => send(box.checked ? '1' : '0')
                return box
            }
            if (values && !values.multi) {
                const select = el('select', {className: 'obj-value'})
                const known = values.values.some(([v]) => v === r.value)
                select.append(...(known ? [] : [[r.value, r.value || '(none)']]).concat(values.values).map(([v, name]) => el('option', {value: v, textContent: name})))
                select.value = r.value
                select.onchange = () => send(select.value)
                return select
            }
            if (values) {
                // A list: a checkbox each; what isn't one of them kept.
                const now = r.value.split(',').map(v => v.trim()).filter(Boolean)
                const others = now.filter(v => !values.values.some(([x]) => x.toLowerCase() === v.toLowerCase()))
                const boxes = el('div', {className: 'obj-flags'})
                for (const [v, name] of values.values) {
                    const box = el('input', {type: 'checkbox', checked: now.some(x => x.toLowerCase() === v.toLowerCase())})
                    box.dataset.value = v
                    box.onchange = () => send([...boxes.querySelectorAll('input:checked')].map(b => b.dataset.value).concat(others).join(','))
                    boxes.append(el('label', {}, box, name))
                }
                return boxes
            }
            const input = el('input', {type: 'text', value: r.value, className: 'obj-value'})
            let timer = 0
            input.oninput = () => {
                clearTimeout(timer)
                timer = setTimeout(() => send(input.value), 400)
            }
            const kinds = this.raw ? undefined : ID_KINDS[r.ty]
            const hint = this.raw ? '' : r.hint
            if (!hint && r.ty !== 'icon' && !kinds) return input
            const box = el('div', {className: 'obj-value-box'}, input)
            if (kinds) {
                // The objects to pick from, by name.
                const pick = el('button', {className: 'obj-pick', textContent: '…', title: 'Pick from the list'})
                pick.onclick = () => this.openPicker(r, kinds, send)
                box.classList.add('with-pick')
                box.prepend(el('div', {className: 'obj-pick-row'}, input, pick))
            }
            if (hint) box.append(el('div', {className: 'obj-hint', textContent: hint}))
            if (r.ty === 'icon' && r.value.trim()) {
                const path = r.value.split(',')[0].trim()
                const canvas = el('canvas', {className: 'obj-icon', width: 32, height: 32})
                canvas.dataset.path = path.toLowerCase()
                const image = this.icons.get(path.toLowerCase())
                if (image) paint(canvas, image)
                else if (image === undefined) {
                    this.icons.set(path.toLowerCase(), null)
                    this.send(this.encode.icons([[255, path]]))
                }
                box.prepend(canvas)
                box.classList.add('with-icon')
            }
            return box
        }

        /**
         * The id picker of field `r`: the objects of `kinds` searched, a
         * click adding one to its list or taking it out (a list's type),
         * or setting it (one id's).
         */
        openPicker(r, kinds, send) {
            this.closePicker()
            const multi = /List$/.test(r.ty)
            let now = r.value.split(',').map(v => v.trim()).filter(Boolean)
            const search = el('input', {type: 'search', placeholder: 'Search', className: 'obj-search'})
            const list = el('div', {className: 'obj-list'})
            const done = el('button', {textContent: 'Done', onclick: () => this.closePicker()})
            const box = el('div', {className: 'obj-picker'}, el('div', {className: 'obj-title', textContent: r.name}), search, list, done)
            const update = () => {
                const words = search.value.toLowerCase().split(/\s+/).filter(Boolean)
                const rows = []
                for (const k of kinds) {
                    const objects = this.lists.get(k)
                    if (!objects) continue
                    for (const o of objects) {
                        if (!words.every(w => `${o.name} ${o.id} ${o.group}`.toLowerCase().includes(w))) continue
                        const on = now.includes(o.id)
                        const row = el('div', {className: `obj-row ${STATES[o.state]}${on ? ' on' : ''}`, textContent: `${o.name} (${o.id})`, title: o.group})
                        row.dataset.id = o.id
                        row.onclick = () => {
                            now = multi ? (on ? now.filter(x => x !== o.id) : [...now, o.id]) : [o.id]
                            send(now.join(','))
                            update()
                            if (!multi) this.closePicker()
                        }
                        rows.push(row)
                        if (rows.length >= 400) break
                    }
                }
                list.replaceChildren(...rows)
            }
            search.oninput = update
            this.picker = {box, update, field: r.field}
            this.root.append(box)
            for (const k of kinds) if (!this.lists.has(k)) this.send(this.encode.objectList(k))
            update()
            search.focus()
        }

        closePicker() {
            this.picker?.box.remove()
            this.picker = null
        }

        /** An id picked in field `field`'s picker (for the tests): as a click would, once listed. */
        pickId(field, level, id) {
            const r = this.fields?.fields.find(r => r.field === field && r.level === (level ?? 0))
            const kinds = r && ID_KINDS[r.ty]
            if (!kinds) return
            const [kind, objectId] = [this.kind, this.fields.id]
            this.openPicker(r, kinds, value => this.send(this.encode.objectSet(kind, objectId, r.field, r.level, r.column, value)))
            const end = Date.now() + 10000
            const click = () => {
                const row = this.picker?.box.querySelector(`[data-id="${CSS.escape(id)}"]`)
                if (row) row.onclick()
                else if (Date.now() < end) setTimeout(click, 100)
            }
            click()
        }

        /** The picked one's fields (`objectFields`). */
        setFields(msg) {
            if (this.pendingNew && msg.objectKind === this.kind && !this.pendingNew.has(msg.id)) {
                this.pendingNew = null
                this.picked = msg.id
                this.renderList()
            }
            if (msg.objectKind !== this.kind || msg.id !== this.picked) return
            this.fields = msg
            this.renderFields()
        }

        renderFields() {
            const f = this.fields
            if (!f || !f.fields.length) {
                this.title.textContent = this.picked ? `${this.picked}: no fields` : 'Pick an object'
                this.table.replaceChildren()
                this.report()
                return
            }
            const own = f.id !== f.base ? ` · based on ${f.base}` : ''
            this.title.textContent = `${f.name} (${f.id})${own}${f.levels > 1 ? ` · ${f.levels} levels` : ''}`
            // A field being typed in keeps what's typed.
            const focused = document.activeElement?.dataset?.key
            const rows = []
            let category = null
            for (const r of f.fields) {
                if (r.category !== category) {
                    category = r.category
                    rows.push(el('div', {className: 'obj-category', textContent: CATEGORIES[category] ?? (category || 'Other')}))
                }
                const key = `${r.field}:${r.level}:${r.column}`
                // Raw data: the field's id (and its level).
                const label = this.raw ? (r.level ? `${r.field} (${r.level})` : r.field) : r.level ? `${r.name} - Level ${r.level}` : r.name
                const name = el('span', {className: 'obj-name', textContent: label, title: `${this.raw ? r.name : r.field} · ${r.ty}${r.changed ? ` · the game's: ${r.default}` : ''}`})
                const [kind, id] = [this.kind, f.id]
                const input = this.control(r, value => this.send(this.encode.objectSet(kind, id, r.field, r.level, r.column, value)))
                input.dataset.key = key
                input.dataset.field = r.field
                input.dataset.level = r.level
                const reset = el('button', {className: 'obj-reset', textContent: '↺', title: `The game's: ${r.default}`, disabled: !r.changed})
                reset.onclick = () => this.send(this.encode.objectSet(this.kind, f.id, r.field, r.level, r.column, ''))
                const row = el('div', {className: `obj-field${r.changed ? ' changed' : ''}`}, name, input, reset)
                if (key === focused) {
                    const old = this.table.querySelector(`[data-key="${CSS.escape(key)}"]`)
                    if (old) row.replaceChild(old, input)
                }
                rows.push(row)
            }
            this.table.replaceChildren(...rows)
            const keep = focused && this.table.querySelector(`[data-key="${CSS.escape(focused)}"]`)
            keep?.focus()
            this.report()
        }

        /** A field typed (for the tests): as the keyboard would. */
        type(field, level, value) {
            const c = [...this.table.querySelectorAll('[data-field]')].find(i => i.dataset.field === field && Number(i.dataset.level) === (level ?? 0))
            if (!c) return false
            if (c.tagName === 'SELECT') {
                c.value = String(value)
                c.onchange()
            } else if (c.type === 'checkbox') {
                c.checked = String(value) === '1'
                c.onchange()
            } else if (c.classList.contains('obj-flags')) {
                const want = String(value).split(',')
                const boxes = [...c.querySelectorAll('input')]
                boxes.forEach(b => (b.checked = want.includes(b.dataset.value)))
                boxes[0]?.onchange()
            } else {
                const input = c.tagName === 'INPUT' ? c : c.querySelector('input[type=text]')
                input.value = String(value)
                input.oninput()
            }
            return true
        }

        report() {
            const f = this.fields
            this.said({
                kind: this.kind,
                count: this.objects.length,
                custom: this.objects.filter(o => o.state === 2).map(o => o.id),
                changed: this.objects.filter(o => o.state === 1).map(o => o.id),
                picked: f && f.id === this.picked ? {id: f.id, base: f.base, name: f.name, levels: f.levels, fields: f.fields.map(r => [r.field, r.level, r.value, r.changed, r.hint, r.ty])} : null,
                raw: this.raw,
                names: [...this.table.querySelectorAll('.obj-name')].slice(0, 3).map(n => n.textContent),
                rows: [...this.list.querySelectorAll('.obj-row')].slice(0, 2).map(n => n.textContent),
                controls: {
                    selects: this.table.querySelectorAll('select').length,
                    checkboxes: this.table.querySelectorAll('input[type=checkbox]').length,
                    icons: [...this.table.querySelectorAll('canvas.obj-icon')].filter(c => c.dataset.painted).length,
                },
            })
        }
    }

    return {Panel, KINDS}
})()
