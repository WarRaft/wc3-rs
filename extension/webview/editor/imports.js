'use strict'

/**
 * The import manager, a panel over the view: the map's imported files
 * (`war3map.imp`), each its path in the map (typed: the file moved there
 * on save), whether the map has it, whether it's to be written on save;
 * Add… (files from disk, through VS Code's dialog), Delete; the one
 * picked looked at: a texture's picture, a sound played, what else it is.
 * The editor's process does each (`Session::import_*`); this only asks.
 */
const EditorImports = (() => {
    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }
    const WHAT = ['a file', 'a texture', 'a sound', 'a model']

    class Panel {
        /**
         * @param root the panel's element
         * @param send sends a frame
         * @param encode the protocol's encoders
         * @param post a message to the extension (`importPick`, and what it shows for the tests)
         */
        constructor(root, send, encode, post) {
            Object.assign(this, {root, send, encode, post})
            this.files = []
            this.picked = null
            this.preview = null
            const add = el('button', {textContent: 'Add…', title: 'Files from disk, imported', onclick: () => this.post({type: 'importPick'})})
            this.deleteButton = el('button', {textContent: 'Delete', title: 'Taken out of the list, and its file out of the map on save', onclick: () => this.picked !== null && this.send(this.encode.importDelete(this.picked))})
            this.list = el('div', {className: 'obj-list imp-list'})
            this.view = el('div', {className: 'imp-view'})
            this.root.append(
                el('div', {className: 'obj-side imp-side'}, el('div', {className: 'obj-title', textContent: 'Imported files'}), this.list, el('div', {className: 'obj-buttons'}, add, this.deleteButton)),
                el('div', {className: 'obj-main'}, this.view),
            )
        }

        open() {
            this.root.hidden = false
            this.send(this.encode.importList())
        }

        close() {
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
        }

        /** The list (`imports`). */
        set(files) {
            this.files = files
            if (this.picked !== null && this.picked >= files.length) {
                this.picked = null
                this.preview = null
            }
            this.render()
            if (this.picked !== null) this.send(this.encode.importPreview(this.picked))
        }

        pick(i) {
            this.picked = i
            this.preview = null
            this.render()
            this.send(this.encode.importPreview(i))
        }

        render() {
            const focused = document.activeElement?.dataset?.import
            this.list.replaceChildren(...this.files.map((f, i) => {
                const path = el('input', {type: 'text', value: f.path, className: 'imp-path', title: f.kept !== f.path ? `kept as ${f.kept}` : ''})
                path.dataset.import = String(i)
                path.onfocus = () => this.picked !== i && this.pick(i)
                path.onchange = () => this.send(this.encode.importSet(i, path.value))
                const mark = el('span', {className: 'imp-mark', textContent: !f.found ? '!' : f.pending ? '•' : '', title: !f.found ? 'The map hasn\'t it' : f.pending ? 'Written on save' : ''})
                const row = el('div', {className: `obj-row imp-row${i === this.picked ? ' on' : ''}${f.found ? '' : ' missing'}`}, mark, path)
                row.onclick = () => this.picked !== i && this.pick(i)
                return row
            }))
            if (focused !== undefined) this.list.querySelector(`[data-import="${focused}"]`)?.focus()
            this.deleteButton.disabled = this.picked === null
            this.renderView()
        }

        /** What the one picked is (`importPreview`). */
        setPreview(msg) {
            if (this.picked === null || this.files[this.picked]?.path !== msg.path) return
            this.preview = msg
            this.renderView()
        }

        renderView() {
            const p = this.preview
            const f = this.picked !== null ? this.files[this.picked] : null
            if (!f) {
                this.view.replaceChildren(el('div', {className: 'obj-title', textContent: this.files.length ? 'Pick a file' : 'The map imports no files'}))
                this.report()
                return
            }
            const parts = [el('div', {className: 'obj-title', textContent: f.path})]
            if (p) {
                parts.push(el('div', {textContent: `${WHAT[p.what] ?? 'a file'}, ${p.size.toLocaleString()} bytes${p.error ? ` · ${p.error}` : ''}`}))
                if (p.picture.width) {
                    const canvas = el('canvas', {className: 'imp-picture', width: p.picture.width, height: p.picture.height})
                    const rgba = p.picture.rgba
                    canvas.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(rgba.buffer, rgba.byteOffset, rgba.byteLength), p.picture.width, p.picture.height), 0, 0)
                    parts.push(canvas)
                }
                if (p.what === 2) parts.push(el('button', {textContent: 'Play', onclick: () => this.send(this.encode.soundPlay(f.path))}))
            }
            this.view.replaceChildren(...parts)
            this.report()
        }

        report() {
            const p = this.preview
            this.post({type: 'imports', imports: {
                files: this.files.map(f => [f.path, f.found, f.pending]),
                picked: this.picked,
                preview: p && {path: p.path, what: p.what, size: p.size, width: p.picture.width, error: p.error},
            }})
        }
    }

    return {Panel}
})()
