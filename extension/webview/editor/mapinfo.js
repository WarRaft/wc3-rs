'use strict'

/**
 * The map's info (`war3map.w3i`), a panel over the view, as World
 * Editor's Scenario menu has it: its description, loading screen and
 * prologue, options (flags), fog, weather, light and water, players and
 * forces. Each field set as it's typed (`InfoSet`); the editor reads and
 * checks it (`Session::info_set`), and a text that's a string of the
 * map's goes into `war3map.wts`.
 */
const EditorMapInfo = (() => {
    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }
    /** `w3i_rs::FLAGS`, World Editor's names. */
    const FLAGS = [
        [0x1, 'Hide minimap in preview screens'], [0x2, 'Modify ally priorities'], [0x4, 'Melee map'],
        [0x10, 'Masked areas are partially visible'], [0x20, 'Fixed player settings for custom forces'], [0x40, 'Use custom forces'],
        [0x80, 'Use custom tech tree'], [0x100, 'Use custom abilities'], [0x200, 'Use custom upgrades'],
        [0x800, 'Show water waves on cliff shores'], [0x1000, 'Show water waves on rolling shores'], [0x2000, 'Use terrain fog'],
        [0x4000, 'Requires The Frozen Throne'], [0x8000, 'Use item classification system'], [0x10000, 'Use water tinting color'],
    ]
    const FORCE_FLAGS = [[0x1, 'Allied'], [0x2, 'Allied victory'], [0x8, 'Share vision'], [0x10, 'Share unit control'], [0x20, 'Share advanced unit control']]
    const KINDS = [[1, 'User'], [2, 'Computer'], [3, 'Neutral'], [4, 'Rescuable']]
    const RACES = [[0, 'Selectable'], [1, 'Human'], [2, 'Orc'], [3, 'Undead'], [4, 'Night Elf']]
    const FOG = [[0, 'None'], [1, 'Linear'], [2, 'Exponential'], [3, 'Exponential squared']]
    /** `input` with its field's name, for the tests. */
    const tag = (input, field) => {
        input.dataset.field = field
        return input
    }
    const hex = rgba => '#' + rgba.split(',').slice(0, 3).map(v => Number(v).toString(16).padStart(2, '0')).join('')

    class Panel {
        /**
         * @param nameOf `(id)`: an object's name (the object editor's lists), or undefined
         */
        constructor(root, send, encode, said, nameOf = () => undefined) {
            Object.assign(this, {root, send, encode, said, nameOf})
            this.fields = new Map()
            this.body = el('div', {className: 'info-body'})
            this.root.append(el('div', {className: 'info-head'}, el('span', {className: 'obj-title', textContent: 'Map info'})), this.body)
        }

        open() {
            this.root.hidden = false
            this.send(this.encode.infoAsk())
        }

        close() {
            this.root.hidden = true
        }

        get isOpen() {
            return !this.root.hidden
        }

        /** The info (`info`): shown; what's being typed kept. */
        set(list) {
            this.fields = new Map(list)
            if (this.root.contains(document.activeElement) && document.activeElement.dataset.field) {
                this.said(this.report())
                return
            }
            this.render()
        }

        get(k) {
            return this.fields.get(k) ?? ''
        }

        setField(field, value) {
            this.send(this.encode.infoSet(field, String(value)))
        }

        /** A text, a number, a choice or a checkbox of `field`. */
        text(field, label, multi = false) {
            const input = el(multi ? 'textarea' : 'input', {value: this.get(field), rows: multi ? 4 : undefined})
            input.dataset.field = field
            let timer = 0
            input.oninput = () => {
                clearTimeout(timer)
                timer = setTimeout(() => this.setField(field, input.value), 500)
            }
            return el('label', {className: multi ? 'info-field info-multi' : 'info-field'}, el('span', {textContent: label}), input)
        }

        choice(field, label, options) {
            const select = el('select')
            select.append(...options.map(([v, t]) => el('option', {value: String(v), textContent: t})))
            select.value = this.get(field)
            select.dataset.field = field
            select.onchange = () => this.setField(field, select.value)
            return el('label', {className: 'info-field'}, el('span', {textContent: label}), select)
        }

        bits(field, list, label) {
            const now = Number(this.get(field))
            const box = el('div', {className: 'info-bits'})
            box.dataset.field = field
            for (const [bit, name] of list) {
                const c = el('input', {type: 'checkbox', checked: (now & bit) !== 0})
                c.dataset.bit = String(bit)
                c.onchange = () => {
                    const v = [...box.querySelectorAll('input')].reduce((n, b) => (b.checked ? n | Number(b.dataset.bit) : n & ~Number(b.dataset.bit)), now)
                    this.setField(field, v >>> 0)
                }
                box.append(el('label', {}, c, name))
            }
            return label ? el('div', {className: 'info-field'}, el('span', {textContent: label}), box) : box
        }

        color(field, label) {
            const now = this.get(field)
            const input = el('input', {type: 'color', value: hex(now)})
            input.dataset.field = field
            input.onchange = () => {
                const [r, g, b] = [1, 3, 5].map(i => parseInt(input.value.slice(i, i + 2), 16))
                this.setField(field, `${r},${g},${b},${now.split(',')[3] ?? 255}`)
            }
            return el('label', {className: 'info-field'}, el('span', {textContent: label}), input)
        }

        /** The camera bounds by their sides, within the map; or from the playable area, as World Editor works them out. */
        cameraBounds() {
            const reset = el('button', {textContent: 'From the playable area'})
            reset.dataset.field = 'camera.reset'
            reset.onclick = () => this.setField('camera.reset', '')
            const [x0, y0, x1, y1] = this.get('camera.extent').split(',')
            return this.section('Camera bounds',
                el('div', {className: 'info-note', textContent: `The map: ${x0}, ${y0} to ${x1}, ${y1}`}),
                this.text('camera.left', 'Left'), this.text('camera.right', 'Right'), this.text('camera.bottom', 'Bottom'), this.text('camera.top', 'Top'), reset)
        }

        section(title, ...children) {
            return el('div', {className: 'info-section'}, el('div', {className: 'obj-category', textContent: title}), ...children)
        }

        render() {
            const tft = this.get('tft') === '1'
            const lines = k => this.get(k).split('\n').filter(Boolean).map(l => l.split('\t'))
            const players = [...this.fields.keys()].filter(k => /^player\.\d+\.name$/.test(k)).map(k => Number(k.split('.')[1]))
            const forces = [...this.fields.keys()].filter(k => /^force\.\d+\.name$/.test(k)).map(k => Number(k.split('.')[1]))
            const parts = [
                this.section('Description', this.text('name', 'Name'), this.text('author', 'Author'), this.text('players_recommended', 'Suggested players'), this.text('description', 'Description', true)),
                this.section('Loading screen', this.text('loading_screen', 'Campaign screen (-1 none)'), ...(tft ? [this.text('loading_screen_model', 'Custom model')] : []), this.text('loading_title', 'Title'), this.text('loading_subtitle', 'Subtitle'), this.text('loading_text', 'Text', true)),
                this.section('Prologue', ...(tft ? [this.text('prologue_path', 'Custom screen')] : []), this.text('prologue_title', 'Title'), this.text('prologue_subtitle', 'Subtitle'), this.text('prologue_text', 'Text', true)),
                this.section('Options', this.bits('flags', FLAGS)),
                this.cameraBounds(),
            ]
            if (tft) {
                parts.push(this.section('Environment',
                    this.choice('fog_style', 'Fog', FOG), this.text('fog_start', 'Fog start (z)'), this.text('fog_end', 'Fog end (z)'), this.text('fog_density', 'Fog density'), this.color('fog_color', 'Fog colour'),
                    this.choice('weather', 'Weather', [['', 'None'], ...lines('weathers')]),
                    this.choice('light_tileset', 'Lighting', [['', 'The map\'s tileset\'s'], ...lines('tilesets')]),
                    this.text('sound_environment', 'Sound environment'),
                    this.color('water_color', 'Water tint')))
            }
            // A player added by its number (12 slots, 24 since 1.32), one taken out.
            const most = Number(this.get('version')) >= 31 ? 24 : 12
            const taken = new Set(players.map(p => Number(this.get(`player.${p}.number`))))
            const add = el('select')
            add.append(el('option', {value: '', textContent: 'Add a player…'}), ...[...Array(most).keys()].filter(n => !taken.has(n)).map(n => el('option', {value: String(n), textContent: `Player ${n + 1}`})))
            add.dataset.field = 'player.add'
            add.onchange = () => add.value !== '' && this.setField('player.add', add.value)
            const remove = field => {
                const b = el('button', {textContent: 'Remove', className: 'info-remove'})
                b.dataset.field = field
                b.onclick = () => this.setField(field, '')
                return b
            }
            const addForce = el('button', {textContent: 'Add force'})
            addForce.dataset.field = 'force.add'
            addForce.onclick = () => this.setField('force.add', '')
            parts.push(this.section('Players', add, ...players.map(n => el('div', {className: 'info-player'},
                el('div', {className: 'info-head'}, el('span', {className: 'info-number', textContent: `Player ${Number(this.get(`player.${n}.number`)) + 1}`}), remove(`player.${n}.remove`)),
                this.text(`player.${n}.name`, 'Name'),
                this.choice(`player.${n}.kind`, 'Controller', KINDS),
                this.choice(`player.${n}.race`, 'Race', RACES),
                this.bits(`player.${n}.fixed_start`, [[1, 'Fixed start location']])))))
            parts.push(this.section('Forces', addForce, ...forces.map(n => el('div', {className: 'info-player'},
                el('div', {className: 'info-head'}, el('span', {className: 'info-number', textContent: `Force ${n + 1}`}), remove(`force.${n}.remove`)),
                this.text(`force.${n}.name`, 'Name'),
                this.bits(`force.${n}.flags`, FORCE_FLAGS, 'Flags'),
                this.bits(`force.${n}.players`, players.map(p => {
                    const number = Number(this.get(`player.${p}.number`))
                    return [2 ** number, this.get(`player.${p}.name`) || `Player ${number + 1}`]
                }), 'Players')))))
            parts.push(this.techTree(players))
            parts.push(this.randomGroups(remove))
            if (tft) parts.push(this.itemTables(remove))
            this.body.replaceChildren(...parts)
            this.said(this.report())
        }

        /** An id typed: sent once it's whole (four characters) or empty. */
        idInput(value, done) {
            const input = el('input', {value, className: 'thing-id', maxLength: 4, placeholder: '----'})
            let timer = 0
            input.oninput = () => {
                clearTimeout(timer)
                if (input.value.length === 0 || input.value.length === 4) timer = setTimeout(() => done(input.value.trim()), 300)
            }
            return input
        }

        number(value, title, done) {
            const input = el('input', {type: 'number', min: 0, max: 100, value, className: 'thing-number', title})
            input.onchange = () => done(input.value)
            return input
        }

        /** The map's players' checkboxes over `mask`, a bit each by their numbers; `done(mask)`. */
        playerBits(players, mask, done) {
            const box = el('div', {className: 'info-players'})
            for (const p of players) {
                const number = Number(this.get(`player.${p}.number`))
                const c = el('input', {type: 'checkbox', checked: (mask & 2 ** number) !== 0, title: this.get(`player.${p}.name`) || `Player ${number + 1}`})
                c.onchange = () => done(c.checked ? (mask | 2 ** number) >>> 0 : (mask & ~(2 ** number)) >>> 0)
                box.append(el('label', {}, c, String(number + 1)))
            }
            return box
        }

        /** The tech tree (what's unavailable, to whom) and the upgrades (a level each: unavailable, available, researched; to whom). */
        techTree(players) {
            const all = players.reduce((m, p) => (m | 2 ** Number(this.get(`player.${p}.number`))) >>> 0, 0)
            const named = id => el('span', {className: 'thing-name', textContent: id ? this.nameOf(id) ?? '?' : ''})
            const techs = this.get('techs').split(';').filter(Boolean).map(t => {
                const [id, mask = '0'] = t.split(':')
                return {id, mask: Number(mask)}
            })
            const sendTechs = () => this.setField('techs', techs.filter(t => t.id).map(t => `${t.id}:${t.mask}`).join(';'))
            const addTech = tag(this.idInput('', v => {
                if (!v) return
                techs.push({id: v, mask: all})
                sendTechs()
            }), 'techs.add')
            addTech.placeholder = '+ id'
            const techRows = techs.map(t => el('div', {className: 'thing-row'},
                this.idInput(t.id, v => {
                    t.id = v
                    sendTechs()
                }), named(t.id),
                this.playerBits(players, t.mask, m => {
                    t.mask = m
                    sendTechs()
                }),
                el('button', {textContent: '×', onclick: () => {
                    techs.splice(techs.indexOf(t), 1)
                    sendTechs()
                }})))
            const ups = this.get('upgrades').split(';').filter(Boolean).map(u => {
                const [id, level = '0', availability = '0', mask = '0'] = u.split(':')
                return {id, level, availability, mask: Number(mask)}
            })
            const sendUps = () => this.setField('upgrades', ups.filter(u => u.id).map(u => `${u.id}:${u.level}:${u.availability}:${u.mask}`).join(';'))
            const addUp = tag(this.idInput('', v => {
                if (!v) return
                ups.push({id: v, level: '0', availability: '0', mask: all})
                sendUps()
            }), 'upgrades.add')
            addUp.placeholder = '+ id'
            const upRows = ups.map((u, i) => {
                const level = el('input', {type: 'number', min: 1, value: Number(u.level) + 1, className: 'thing-number', title: 'Level'})
                level.onchange = () => {
                    u.level = String(Math.max(0, Number(level.value) - 1))
                    sendUps()
                }
                const availability = el('select')
                availability.append(...[['0', 'Unavailable'], ['1', 'Available'], ['2', 'Researched']].map(([v, t]) => el('option', {value: v, textContent: t})))
                availability.value = u.availability
                availability.dataset.field = `upgrades.${i}.availability`
                availability.onchange = () => {
                    u.availability = availability.value
                    sendUps()
                }
                return el('div', {className: 'thing-row info-wrap'},
                    this.idInput(u.id, v => {
                        u.id = v
                        sendUps()
                    }), named(u.id), level, availability,
                    this.playerBits(players, u.mask, m => {
                        u.mask = m
                        sendUps()
                    }),
                    el('button', {textContent: '×', onclick: () => {
                        ups.splice(ups.indexOf(u), 1)
                        sendUps()
                    }}))
            })
            return this.section('Tech tree',
                el('div', {className: 'thing-sub', textContent: 'Unavailable to the players checked'}), ...techRows, el('div', {className: 'thing-row'}, addTech),
                el('div', {className: 'obj-category', textContent: 'Upgrades'}), ...upRows, el('div', {className: 'thing-row'}, addUp))
        }

        /** The random groups: each its name, its columns' kinds, its rows (a chance, an id a column). */
        randomGroups(remove) {
            const groups = [...this.fields.keys()].filter(k => /^unit_table\.\d+\.name$/.test(k)).map(k => Number(k.split('.')[1]))
            const add = el('button', {textContent: 'Add random group'})
            add.dataset.field = 'unit_table.add'
            add.onclick = () => this.setField('unit_table.add', '')
            const KINDS = [[0, 'Unit'], [1, 'Building'], [2, 'Item']]
            return this.section('Random groups', add, ...groups.map(n => {
                const f = `unit_table.${n}`
                const columns = this.get(`${f}.columns`).split(',').filter(Boolean)
                const rows = this.get(`${f}.rows`).split(';').filter(Boolean).map(r => {
                    const [chance, ids = ''] = r.split(':')
                    return {chance, ids: ids.split(',')}
                })
                const sendColumns = () => this.setField(`${f}.columns`, columns.join(','))
                const sendRows = () => this.setField(`${f}.rows`, rows.map(r => `${r.chance}:${r.ids.join(',')}`).join(';'))
                const head = el('div', {className: 'thing-row'}, el('span', {textContent: 'Chance'}), ...columns.map((c, i) => {
                    const select = el('select')
                    select.append(...KINDS.map(([v, t]) => el('option', {value: String(v), textContent: t})))
                    select.value = c
                    select.onchange = () => {
                        columns[i] = select.value
                        sendColumns()
                    }
                    return select
                }), el('button', {textContent: '+ column', onclick: () => {
                    columns.push('0')
                    sendColumns()
                }}), ...(columns.length > 1 ? [el('button', {textContent: '− column', onclick: () => {
                    columns.pop()
                    sendColumns()
                }})] : []))
                const lines = rows.map((r, ri) => el('div', {className: 'thing-row'},
                    tag(this.number(r.chance, 'Chance (%)', v => {
                        r.chance = v
                        sendRows()
                    }), `${f}.row.${ri}.chance`),
                    ...columns.map((_, i) => tag(this.idInput(r.ids[i] ?? '', v => {
                        r.ids[i] = v
                        sendRows()
                    }), `${f}.row.${ri}.${i}`)),
                    el('button', {textContent: '×', onclick: () => {
                        rows.splice(rows.indexOf(r), 1)
                        sendRows()
                    }})))
                const addRow = el('button', {textContent: '+ row'})
                addRow.dataset.field = `${f}.addRow`
                addRow.onclick = () => {
                    rows.push({chance: '0', ids: columns.map(() => '')})
                    sendRows()
                }
                return el('div', {className: 'info-player'},
                    el('div', {className: 'info-head'}, el('span', {className: 'info-number', textContent: `Group ${n + 1}`}), remove(`${f}.remove`)),
                    this.text(`${f}.name`, 'Name'), head, ...lines, addRow)
            }))
        }

        /** The item tables: each its name, its sets of items and their chances. */
        itemTables(remove) {
            const tables = [...this.fields.keys()].filter(k => /^item_table\.\d+\.name$/.test(k)).map(k => Number(k.split('.')[1]))
            const add = el('button', {textContent: 'Add item table'})
            add.dataset.field = 'item_table.add'
            add.onclick = () => this.setField('item_table.add', '')
            return this.section('Item tables', add, ...tables.map(n => {
                const f = `item_table.${n}`
                const sets = this.get(`${f}.sets`).split(';').filter(Boolean).map(set => set.split(',').filter(Boolean).map(e => {
                    const [id, chance = '100'] = e.split(':')
                    return {id, chance}
                }))
                const out = () => this.setField(`${f}.sets`, sets.map(set => set.filter(e => e.id).map(e => `${e.id}:${e.chance}`).join(',')).filter(Boolean).join(';'))
                const blocks = sets.map((set, i) => el('div', {className: 'thing-set'},
                    el('div', {className: 'thing-sub', textContent: `Set ${i + 1} · ${set.reduce((t, e) => t + Number(e.chance || 0), 0)}%`}),
                    ...set.map(e => el('div', {className: 'thing-row'},
                        this.idInput(e.id, v => {
                            e.id = v
                            out()
                        }),
                        this.number(e.chance, 'Chance (%)', v => {
                            e.chance = v
                            out()
                        }),
                        el('button', {textContent: '×', onclick: () => {
                            set.splice(set.indexOf(e), 1)
                            out()
                        }}))),
                    el('div', {className: 'thing-row'}, Object.assign(this.idInput('', v => {
                        if (!v) return
                        set.push({id: v, chance: '100'})
                        out()
                    }), {placeholder: '+ id'}))))
                const newSet = Object.assign(this.idInput('', v => {
                    if (!v) return
                    sets.push([{id: v, chance: '100'}])
                    out()
                }), {placeholder: '+ set: id'})
                newSet.dataset.field = `${f}.newSet`
                return el('div', {className: 'info-player'},
                    el('div', {className: 'info-head'}, el('span', {className: 'info-number', textContent: `Table ${n + 1}`}), remove(`${f}.remove`)),
                    this.text(`${f}.name`, 'Name'), ...blocks, el('div', {className: 'thing-row'}, newSet))
            }))
        }

        /** A field set as the page would (for the tests): a text typed, a choice made, a bit checked. */
        type(field, value, bit) {
            const c = this.body.querySelector(`[data-field="${CSS.escape(field)}"]`)
            if (!c) return
            if (c.tagName === 'BUTTON') {
                c.onclick()
            } else if (c.classList.contains('info-bits')) {
                const box = c.querySelector(`input[data-bit="${bit}"]`)
                box.checked = !!value
                box.onchange()
            } else if (c.tagName === 'SELECT' || c.type === 'color') {
                c.value = String(value)
                c.onchange()
            } else {
                c.value = String(value)
                ;(c.oninput ?? c.onchange)()
            }
        }

        report() {
            return Object.fromEntries([...this.fields].filter(([k]) => k !== 'weathers' && k !== 'tilesets'))
        }
    }

    return {Panel}
})()
