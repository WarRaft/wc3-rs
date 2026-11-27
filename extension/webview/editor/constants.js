'use strict'

/**
 * The map's gameplay constants (`war3mapMisc.txt`) or game interface
 * (`war3mapSkin.txt`), a panel over the view each, as World Editor's
 * Advanced > Gameplay Constants and Game Interface have them: Use Custom
 * ... (the map's own file, or the game's), then each field by its name,
 * sorted, searched; its value set as it's typed (`ConstantSet`: the
 * editor checks it by its type and limits; a string of the map's set
 * in `war3map.wts`), the ones the map changes marked, ↺ the game's again.
 */
const EditorConstants = (() => {
    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }

    const paint = (canvas, image) => {
        if (!image) return
        canvas.width = image.width
        canvas.height = image.height
        canvas.getContext('2d').putImageData(image, 0, 0)
        canvas.dataset.painted = '1'
    }

    /** Whether two values are the same: as numbers when they're numbers (`0` and `0.0`), item by item (`constants::same`). */
    const same = (a, b) => {
        const [x, y] = [a.split(','), b.split(',')].map(l => l.map(i => i.trim()))
        return x.length === y.length && x.every((v, i) => (v !== '' && y[i] !== '' && !isNaN(v) && !isNaN(y[i]) ? Math.abs(Number(v) - Number(y[i])) < 1e-6 : v.toLowerCase() === y[i].toLowerCase()))
    }

    class Panel {
        /**
         * @param book 0 the gameplay constants, 1 the game interface (`constants::BOOKS`)
         * @param said `(report)`: what it shows, for the tests
         */
        constructor(root, book, title, send, encode, said) {
            Object.assign(this, {root, book, send, encode, said})
            this.custom = false
            this.fields = []
            /** The icons read by path (lower case; `null` asked for, or none). */
            this.icons = new Map()
            this.filter = ''
            this.useCustom = el('input', {type: 'checkbox'})
            this.useCustom.dataset.field = 'custom'
            this.useCustom.onchange = () => this.set('custom', this.useCustom.checked ? '1' : '0')
            this.search = el('input', {type: 'search', placeholder: 'Search'})
            this.search.oninput = () => {
                this.filter = this.search.value.toLowerCase()
                this.render()
            }
            this.body = el('div', {className: 'info-body constants-body'})
            this.root.append(
                el('div', {className: 'info-head'}, el('span', {className: 'obj-title', textContent: title})),
                el('div', {className: 'constants-bar'}, el('label', {}, this.useCustom, `Use custom ${title.toLowerCase()}`), this.search),
                this.body,
            )
        }

        open() {
            this.root.hidden = false
            this.send(this.encode.constantsAsk(this.book))
        }

        close() {
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
        }

        set(id, value) {
            this.send(this.encode.constantSet(this.book, id, String(value)))
        }

        /** The constants (`Constants`): shown; what's being typed kept. */
        setAll(custom, fields) {
            this.custom = custom
            this.fields = [...fields].sort((a, b) => a.name.localeCompare(b.name))
            this.said(this.report())
            if (this.root.contains(document.activeElement) && document.activeElement.dataset.field && document.activeElement !== this.useCustom) return
            this.render()
        }

        report() {
            const painted = this.root.querySelectorAll('canvas.obj-icon[data-painted]').length
            const name = id => this.fields.find(f => f.id === id)?.name
            return {custom: this.custom, count: this.fields.length, changed: this.fields.filter(f => !same(f.value, f.game)).map(f => [f.id, f.value]), icons: painted, names: ['f001', 'e001'].map(name)}
        }

        /** An icon read (`Icons` of kind 255): drawn where it's shown. */
        setIcon(path, image) {
            const key = path.toLowerCase()
            if (!this.icons.has(key)) return
            this.icons.set(key, image)
            for (const c of this.root.querySelectorAll('canvas.obj-icon')) if (c.dataset.path === key) paint(c, image)
            this.said(this.report())
        }

        render() {
            this.useCustom.checked = this.custom
            const shown = this.fields.filter(f => !this.filter || f.name.toLowerCase().includes(this.filter) || f.key.toLowerCase().includes(this.filter))
            this.body.replaceChildren(...shown.map(f => this.row(f)))
        }

        /** A constant's row: its name, its value (a checkbox for a `bool`), ↺. */
        row(f) {
            const changed = !same(f.value, f.game)
            let input
            if (f.ty === 'bool') {
                input = el('input', {type: 'checkbox', checked: f.value === '1', disabled: !this.custom})
                input.onchange = () => this.set(f.id, input.checked ? '1' : '0')
            } else {
                input = el('input', {value: f.value, disabled: !this.custom, title: [f.min && `${f.min} at least`, f.max && `${f.max} at most`].filter(Boolean).join(', ')})
                let timer = 0
                input.oninput = () => {
                    clearTimeout(timer)
                    timer = setTimeout(() => this.set(f.id, input.value), 500)
                }
            }
            input.dataset.field = f.id
            // An icon's picture beside it, asked for once.
            let picture = el('span')
            if (f.ty === 'icon' && f.value.trim()) {
                const path = f.value.trim().toLowerCase()
                picture = el('canvas', {className: 'obj-icon', width: 32, height: 32})
                picture.dataset.path = path
                const image = this.icons.get(path)
                if (image) paint(picture, image)
                else if (image === undefined) {
                    this.icons.set(path, null)
                    this.send(this.encode.icons([[255, f.value.trim()]]))
                }
            }
            const reset = el('button', {textContent: '↺', title: `The game's: ${f.game}`, disabled: !this.custom || !changed, onclick: () => this.set(f.id, f.game)})
            return el('label', {className: changed ? 'info-field constant changed' : 'info-field constant', title: f.key}, el('span', {textContent: f.name}), picture, input, reset)
        }

        /** A constant typed as its input would be: for the tests. */
        type(id, value) {
            const input = this.root.querySelector(`[data-field="${CSS.escape(id)}"]`)
            if (!input) return
            if (input.type === 'checkbox') {
                input.checked = value === '1' || value === true
                input.onchange()
            } else {
                input.value = String(value)
                this.set(id, input.value)
            }
        }
    }

    return {Panel}
})()
