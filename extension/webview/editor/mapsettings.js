'use strict'

/**
 * The map's settings, a panel over the view: its size (cells added or
 * taken off each side, the playable area's margins), its tiles (the
 * tile setter: the list, its order, the tileset; a tile taken out goes
 * to the one picked for it) and their pathing (the tile pather). The
 * editor's process does each (`editor/src/resize.rs`, `tiles.rs`) and
 * sends the map again; this only asks.
 */
const EditorMapSettings = (() => {
    const FLAGS = [[2, 'No walking'], [4, 'No flying'], [8, 'No building']]

    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }
    const number = (value, title) => el('input', {type: 'number', value: String(value), step: '1', title, className: 'map-number'})

    class Panel {
        /**
         * @param root the panel's element
         * @param send sends a frame
         * @param encode the protocol's encoders
         */
        constructor(root, send, encode) {
            this.root = root
            this.send = send
            this.encode = encode
            this.settings = null
            /** The map's tiles as the setter has them: {id, from} (from: its old index, or -1 for one added). */
            this.list = []
            /** Ground icons by id (the palette's). */
            this.icons = new Map()
            /** Cells added left and below by the last resize asked for: the view follows. */
            this.shift = null
        }

        open() {
            this.root.hidden = false
            this.send(this.encode.mapAsk())
        }

        close() {
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
        }

        setIcons(grounds) {
            for (const g of grounds) if (g.icon.width) this.icons.set(g.id, g.icon)
            if (this.settings) this.render()
        }

        set(msg) {
            this.settings = msg
            this.list = msg.mapTiles.map((id, from) => ({id, from}))
            this.tileset = msg.tileset
            if (this.isOpen) this.render()
        }

        name(id) {
            return this.settings.tiles.find(t => t.id === id)?.name ?? id
        }

        icon(id) {
            const icon = this.icons.get(id)
            if (!icon) return el('span', {className: 'map-tile-icon'})
            const c = el('canvas', {width: icon.width, height: icon.height, className: 'map-tile-icon'})
            c.getContext('2d').putImageData(new ImageData(new Uint8ClampedArray(icon.rgba.buffer, icon.rgba.byteOffset, icon.rgba.byteLength), icon.width, icon.height), 0, 0)
            return c
        }

        render() {
            const s = this.settings
            const close = el('button', {textContent: '✕', title: 'Close', className: 'map-close', onclick: () => this.close()})
            this.root.replaceChildren(el('div', {className: 'map-head'}, el('b', {textContent: 'Map'}), close), this.sizeSection(s), this.tilesSection(s), this.pathingSection(s))
        }

        sizeSection(s) {
            const sides = ['left', 'right', 'bottom', 'top'].map(k => [k, number(0, `Cells added on the ${k} (fewer than 0: taken off)`)])
            const margins = s.margins.map((m, i) => number(m, `The playable area's margin, ${['left', 'right', 'bottom', 'top'][i]}`))
            const grid = (inputs, labels) => el('div', {className: 'map-grid'}, ...inputs.flatMap((input, i) => [el('span', {textContent: labels[i]}), input]))
            const apply = el('button', {
                textContent: 'Resize',
                onclick: () => {
                    const v = Object.fromEntries(sides.map(([k, input]) => [k, Number(input.value) | 0]))
                    const m = margins.map(input => Number(input.value) | 0)
                    const changed = m.some((x, i) => x !== s.margins[i])
                    this.resize(v, changed ? m : null)
                },
            })
            return el('section', {className: 'map-section'},
                el('h3', {textContent: `Size: ${s.width - 1} × ${s.height - 1} cells`}),
                el('div', {className: 'map-hint', textContent: 'Cells added (fewer than 0: taken off). Nothing can be undone over it.'}),
                grid(sides.map(([, i]) => i), ['Left', 'Right', 'Bottom', 'Top']),
                ...(s.info ? [el('div', {className: 'map-hint', textContent: 'The playable area\'s margins'}), grid(margins, ['Left', 'Right', 'Bottom', 'Top'])] : []),
                apply)
        }

        resize(sides, margins) {
            this.shift = [sides.left, sides.bottom]
            this.send(this.encode.resize(sides, margins))
        }

        tilesSection(s) {
            const removed = s.mapTiles.map((id, i) => i).filter(i => !this.list.some(t => t.from === i))
            const rows = this.list.map((t, i) => {
                const move = (d, label) => el('button', {
                    textContent: label,
                    disabled: i + d < 0 || i + d >= this.list.length,
                    onclick: () => {
                        [this.list[i], this.list[i + d]] = [this.list[i + d], this.list[i]]
                        this.render()
                    },
                })
                const remove = el('button', {
                    textContent: '✕', title: 'Take it out', disabled: this.list.length <= 1,
                    onclick: () => {
                        this.list.splice(i, 1)
                        this.render()
                    },
                })
                return el('div', {className: 'map-tile'}, this.icon(t.id), el('span', {textContent: this.name(t.id), title: t.id, className: 'map-tile-name'}), move(-1, '↑'), move(1, '↓'), remove)
            })
            // Where a tile taken out goes.
            const goes = removed.map(i => {
                const select = el('select', {}, ...this.list.map((t, k) => el('option', {value: String(k), textContent: this.name(t.id)})))
                select.dataset.from = String(i)
                return el('label', {className: 'map-goes'}, `${this.name(s.mapTiles[i])} → `, select)
            })
            const tilesets = el('select', {}, ...s.tilesets.map(t => el('option', {value: t.letter, textContent: `${t.name} (${t.letter})`, selected: t.letter === this.tileset})))
            tilesets.onchange = () => {
                this.tileset = tilesets.value
                this.render()
            }
            const available = s.tiles.filter(t => t.id[0] === this.tileset && !this.list.some(l => l.id === t.id))
            const add = el('select', {}, ...available.map(t => el('option', {value: t.id, textContent: `${t.name}${t.cliff ? ' (cliff)' : ''}`})))
            const addButton = el('button', {
                textContent: 'Add',
                disabled: !available.length || this.list.length >= s.maxTiles,
                onclick: () => {
                    this.list.push({id: add.value, from: -1})
                    this.render()
                },
            })
            const apply = el('button', {
                textContent: 'Set tiles',
                onclick: () => {
                    const to = s.mapTiles.map((id, i) => {
                        const k = this.list.findIndex(t => t.from === i)
                        if (k >= 0) return k
                        const select = this.root.querySelector(`select[data-from="${i}"]`)
                        return select ? Number(select.value) : 0
                    })
                    this.send(this.encode.tiles(this.list.map(t => t.id), to, this.tileset))
                },
            })
            return el('section', {className: 'map-section'},
                el('h3', {textContent: `Tiles: ${this.list.length} / ${s.maxTiles}`}),
                el('label', {className: 'map-row'}, 'Tileset ', tilesets),
                ...rows,
                ...(goes.length ? [el('div', {className: 'map-hint', textContent: 'Where the tiles taken out go:'}), ...goes] : []),
                el('div', {className: 'map-row'}, add, addButton),
                apply)
        }

        pathingSection(s) {
            const tile = el('select', {}, ...s.mapTiles.map((id, i) => el('option', {value: String(i), textContent: this.name(id)})))
            const boxes = FLAGS.map(([bit, label]) => {
                const input = el('input', {type: 'checkbox', value: String(bit)})
                return [input, el('label', {}, input, ` ${label}`)]
            })
            const fill = () => {
                const forbids = s.tiles.find(t => t.id === s.mapTiles[Number(tile.value)])?.pathing ?? 0
                for (const [input] of boxes) input.checked = (forbids & Number(input.value)) !== 0
            }
            tile.onchange = fill
            fill()
            const apply = el('button', {
                textContent: 'Set its pathing',
                onclick: () => this.send(this.encode.tilePathing(Number(tile.value), boxes.reduce((f, [input]) => f | (input.checked ? Number(input.value) : 0), 0))),
            })
            return el('section', {className: 'map-section'},
                el('h3', {textContent: 'Tile pathing'}),
                el('div', {className: 'map-hint', textContent: 'Where the tile lies (no cliff over it), over what\'s painted.'}),
                tile, el('div', {className: 'map-flags'}, ...boxes.map(([, l]) => l)), apply)
        }
    }

    return {Panel}
})()
