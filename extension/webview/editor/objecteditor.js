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
            /** Icons by path (`null`: none), and the canvases waiting for them. */
            this.icons = new Map()
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
                // The new one comes picked (its fields with the list).
                this.pendingNew = true
                this.send(this.encode.objectNew(this.kind, this.base()))
            }})
            this.deleteButton = el('button', {textContent: 'Delete', title: 'A custom object taken away; a standard one\'s changes taken out', onclick: () => this.picked && this.send(this.encode.objectDelete(this.kind, this.picked))})
            this.closeButton = el('button', {textContent: 'Close', onclick: () => this.close()})
            this.title = el('div', {className: 'obj-title'})
            this.table = el('div', {className: 'obj-fields'})
            this.root.append(
                el('div', {className: 'obj-side'}, this.tabs, this.search, this.list, el('div', {className: 'obj-buttons'}, this.newButton, this.deleteButton, this.closeButton)),
                el('div', {className: 'obj-main'}, this.title, this.table),
            )
        }

        open() {
            this.root.hidden = false
            this.showKind(this.kind)
        }

        close() {
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
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
                const row = el('div', {className: `obj-row ${STATES[o.state]}${o.id === this.picked ? ' on' : ''}`, textContent: `${o.name} (${o.id})`, title: o.group})
                row.onclick = () => this.pick(o.id)
                rows.push(row)
            }
            this.list.replaceChildren(...rows)
            this.newButton.disabled = this.deleteButton.disabled = !this.picked
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
            const values = this.enums.get(r.ty)
            if (r.ty === 'bool') {
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
            if (!r.hint && r.ty !== 'icon') return input
            const box = el('div', {className: 'obj-value-box'}, input)
            if (r.hint) box.append(el('div', {className: 'obj-hint', textContent: r.hint}))
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

        /** The picked one's fields (`objectFields`). */
        setFields(msg) {
            if (this.pendingNew && msg.objectKind === this.kind) {
                this.pendingNew = false
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
                const name = el('span', {className: 'obj-name', textContent: r.level ? `${r.name} - Level ${r.level}` : r.name, title: `${r.field} · ${r.ty}${r.changed ? ` · the game's: ${r.default}` : ''}`})
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
