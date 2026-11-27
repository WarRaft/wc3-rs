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
    const hex = rgba => '#' + rgba.split(',').slice(0, 3).map(v => Number(v).toString(16).padStart(2, '0')).join('')

    class Panel {
        constructor(root, send, encode, said) {
            Object.assign(this, {root, send, encode, said})
            this.fields = new Map()
            this.body = el('div', {className: 'info-body'})
            const close = el('button', {textContent: 'Close', onclick: () => this.close()})
            this.root.append(el('div', {className: 'info-head'}, el('span', {className: 'obj-title', textContent: 'Map info'}), close), this.body)
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
            ]
            if (tft) {
                parts.push(this.section('Environment',
                    this.choice('fog_style', 'Fog', FOG), this.text('fog_start', 'Fog start (z)'), this.text('fog_end', 'Fog end (z)'), this.text('fog_density', 'Fog density'), this.color('fog_color', 'Fog colour'),
                    this.choice('weather', 'Weather', [['', 'None'], ...lines('weathers')]),
                    this.choice('light_tileset', 'Lighting', [['', 'The map\'s tileset\'s'], ...lines('tilesets')]),
                    this.text('sound_environment', 'Sound environment'),
                    this.color('water_color', 'Water tint')))
            }
            parts.push(this.section('Players', ...players.map(n => el('div', {className: 'info-player'},
                el('span', {className: 'info-number', textContent: `Player ${Number(this.get(`player.${n}.number`)) + 1}`}),
                this.text(`player.${n}.name`, 'Name'),
                this.choice(`player.${n}.kind`, 'Controller', KINDS),
                this.choice(`player.${n}.race`, 'Race', RACES),
                this.bits(`player.${n}.fixed_start`, [[1, 'Fixed start location']])))))
            parts.push(this.section('Forces', ...forces.map(n => el('div', {className: 'info-player'},
                this.text(`force.${n}.name`, 'Name'),
                this.bits(`force.${n}.flags`, FORCE_FLAGS, 'Flags'),
                this.bits(`force.${n}.players`, players.map(p => {
                    const number = Number(this.get(`player.${p}.number`))
                    return [2 ** number, this.get(`player.${p}.name`) || `Player ${number + 1}`]
                }), 'Players')))))
            this.body.replaceChildren(...parts)
            this.said(this.report())
        }

        /** A field set as the page would (for the tests): a text typed, a choice made, a bit checked. */
        type(field, value, bit) {
            const c = this.body.querySelector(`[data-field="${CSS.escape(field)}"]`)
            if (!c) return
            if (c.classList.contains('info-bits')) {
                const box = c.querySelector(`input[data-bit="${bit}"]`)
                box.checked = !!value
                box.onchange()
            } else if (c.tagName === 'SELECT' || c.type === 'color') {
                c.value = String(value)
                c.onchange()
            } else {
                c.value = String(value)
                c.oninput()
            }
        }

        report() {
            return Object.fromEntries([...this.fields].filter(([k]) => k !== 'weathers' && k !== 'tilesets'))
        }
    }

    return {Panel}
})()
