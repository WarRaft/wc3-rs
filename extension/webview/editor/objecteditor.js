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
            this.list.replaceChildren(...shown.map(o => {
                const row = el('div', {className: `obj-row ${STATES[o.state]}${o.id === this.picked ? ' on' : ''}`, textContent: `${o.name} (${o.id})`, title: o.group})
                row.onclick = () => this.pick(o.id)
                return row
            }))
            this.newButton.disabled = this.deleteButton.disabled = !this.picked
            this.report()
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
                const input = el('input', {type: 'text', value: r.value, className: 'obj-value'})
                input.dataset.key = key
                input.dataset.field = r.field
                input.dataset.level = r.level
                let timer = 0
                input.oninput = () => {
                    clearTimeout(timer)
                    const [kind, id, value] = [this.kind, f.id, input.value]
                    timer = setTimeout(() => this.send(this.encode.objectSet(kind, id, r.field, r.level, r.column, value)), 400)
                }
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
            const input = [...this.table.querySelectorAll('input')].find(i => i.dataset.field === field && Number(i.dataset.level) === (level ?? 0))
            if (!input) return false
            input.value = String(value)
            input.oninput()
            return true
        }

        report() {
            const f = this.fields
            this.said({
                kind: this.kind,
                count: this.objects.length,
                custom: this.objects.filter(o => o.state === 2).map(o => o.id),
                changed: this.objects.filter(o => o.state === 1).map(o => o.id),
                picked: f && f.id === this.picked ? {id: f.id, base: f.base, name: f.name, levels: f.levels, fields: f.fields.map(r => [r.field, r.level, r.value, r.changed])} : null,
            })
        }
    }

    return {Panel, KINDS}
})()
