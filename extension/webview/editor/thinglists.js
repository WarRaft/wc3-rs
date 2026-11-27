'use strict'

/**
 * A thing's lists in the Select tool's panel, as the editor gives them
 * (`Thing`'s texts, `Session::thing_texts`): a hero's inventory (six
 * slots), the abilities it starts with changed (id, level, autocast), the
 * item sets it drops (items and their chances). Each change sends the
 * list's text whole (`ThingSetText`); the editor checks it.
 */
const EditorThingLists = (() => {
    const el = (tag, props = {}, ...children) => {
        const e = Object.assign(document.createElement(tag), props)
        e.append(...children)
        return e
    }
    /** The object editor's kinds: items 1, abilities 4. */
    const ITEMS = 1
    const ABILITIES = 4
    const UNITS = 0
    /** Item classes a random item is of (`doo_rs::Random::Any`'s class). */
    const CLASSES = ['Any class', 'Permanent', 'Charged', 'Power Up', 'Artifact', 'Purchasable', 'Campaign', 'Miscellaneous']

    /**
     * @param root where the lists go
     * @param send `(field, text)`: a list's text set
     * @param nameOf `(kind, id)`: its name, or undefined
     */
    function render(root, texts, send, nameOf) {
        const named = (kind, id) => {
            const name = id && nameOf(kind, id)
            return el('span', {className: 'thing-name', textContent: name ?? (id ? '?' : '')})
        }
        const idInput = (value, onchange) => {
            const input = el('input', {type: 'text', value, className: 'thing-id', maxLength: 4, placeholder: '----'})
            let timer = 0
            input.oninput = () => {
                clearTimeout(timer)
                if (input.value.length === 0 || input.value.length === 4) timer = setTimeout(onchange, 300)
            }
            return input
        }
        const sections = []
        const groups = (texts.find(([f]) => f === 'random_groups')?.[1] ?? '').split('\n').filter(Boolean).map(l => {
            const [name, columns] = l.split('\t')
            return {name, columns: Number(columns)}
        })
        const unit = texts.some(([f]) => f === 'random_groups')
        for (const [field, text] of texts) {
            if (field === 'random') {
                const [how, ...rest] = text.split(':')
                const head = el('div', {className: 'thing-head', textContent: unit ? 'Random unit' : 'Random item'})
                const mode = el('select', {className: 'thing-mode'})
                mode.append(...[['any', unit ? 'Any of a level' : 'Any of a level and class'], ...(unit ? [['group', 'From a random group'], ['table', 'From a table']] : [])].map(([v, t]) => el('option', {value: v, textContent: t})))
                mode.value = how || 'any'
                mode.dataset.list = 'random:mode'
                mode.onchange = () => send('random', mode.value === 'any' ? (unit ? 'any:-1' : 'any:-1:0') : mode.value === 'group' ? 'group:0:0' : 'table:')
                const rows = [el('div', {className: 'thing-row'}, mode)]
                if ((how || 'any') === 'any') {
                    const level = el('input', {type: 'number', min: -1, value: rest[0] ?? '-1', className: 'thing-number', title: 'Level (-1 any)'})
                    level.dataset.list = 'random:level'
                    const klass = el('select')
                    klass.append(...CLASSES.map((c, i) => el('option', {value: i, textContent: c})))
                    klass.value = rest[1] ?? '0'
                    const out = () => send('random', unit ? `any:${level.value}` : `any:${level.value}:${klass.value}`)
                    level.onchange = out
                    klass.onchange = out
                    rows.push(el('div', {className: 'thing-row'}, el('span', {textContent: 'Level'}), level, ...(unit ? [] : [klass])))
                } else if (how === 'group') {
                    const group = el('select')
                    group.append(...groups.map((g, i) => el('option', {value: i, textContent: g.name || `Group ${i}`})))
                    group.value = rest[0] ?? '0'
                    const column = el('input', {type: 'number', min: 1, value: Number(rest[1] ?? 0) + 1, className: 'thing-number', title: 'Column'})
                    const out = () => send('random', `group:${group.value}:${Number(column.value) - 1}`)
                    group.onchange = out
                    column.onchange = out
                    rows.push(el('div', {className: 'thing-row'}, group, el('span', {textContent: 'Column'}), column))
                } else {
                    const list = rest.join(':').split(',').filter(Boolean).map(e => {
                        const [id, chance = '100'] = e.split(':')
                        return {id, chance}
                    })
                    const out = () => send('random', `table:${list.filter(e => e.id).map(e => `${e.id}:${e.chance}`).join(',')}`)
                    for (const e of list) {
                        const input = idInput(e.id, () => {
                            e.id = input.value.trim()
                            out()
                        })
                        const chance = el('input', {type: 'number', min: 0, max: 100, value: e.chance, className: 'thing-number', title: 'Chance (%)'})
                        chance.onchange = () => {
                            e.chance = chance.value
                            out()
                        }
                        const remove = el('button', {textContent: '×', title: 'Take it out'})
                        remove.onclick = () => {
                            list.splice(list.indexOf(e), 1)
                            out()
                        }
                        rows.push(el('div', {className: 'thing-row'}, input, named(UNITS, e.id), chance, remove))
                    }
                    const add = idInput('', () => {
                        if (!add.value.trim()) return
                        list.push({id: add.value.trim(), chance: '100'})
                        out()
                    })
                    add.placeholder = '+ id'
                    add.dataset.list = 'random:add'
                    rows.push(el('div', {className: 'thing-row'}, add))
                }
                sections.push(el('div', {className: 'thing-list'}, head, ...rows))
                continue
            }
            if (field === 'items') {
                const slots = text.split(',')
                while (slots.length < 6) slots.push('')
                const rows = slots.map((id, s) => {
                    const input = idInput(id, () => {
                        slots[s] = input.value.trim()
                        send('items', slots.join(','))
                    })
                    input.dataset.list = `items:${s}`
                    return el('div', {className: 'thing-row'}, el('span', {textContent: `Slot ${s + 1}`}), input, named(ITEMS, id))
                })
                sections.push(el('div', {className: 'thing-list'}, el('div', {className: 'thing-head', textContent: 'Inventory'}), ...rows))
            } else if (field === 'abilities') {
                const list = text.split(',').filter(Boolean).map(a => {
                    const [id, level = '1', auto = '0'] = a.split(':')
                    return {id, level, auto}
                })
                const out = () => send('abilities', list.filter(a => a.id).map(a => `${a.id}:${a.level}:${a.auto}`).join(','))
                const rows = list.map(a => {
                    const input = idInput(a.id, () => {
                        a.id = input.value.trim()
                        out()
                    })
                    const level = el('input', {type: 'number', min: 1, value: a.level, className: 'thing-number', title: 'Level'})
                    level.onchange = () => {
                        a.level = level.value
                        out()
                    }
                    const auto = el('input', {type: 'checkbox', checked: a.auto === '1', title: 'Autocast on'})
                    auto.onchange = () => {
                        a.auto = auto.checked ? '1' : '0'
                        out()
                    }
                    const remove = el('button', {textContent: '×', title: 'Take it out'})
                    remove.onclick = () => {
                        list.splice(list.indexOf(a), 1)
                        out()
                    }
                    return el('div', {className: 'thing-row'}, input, named(ABILITIES, a.id), level, auto, remove)
                })
                const add = idInput('', () => {
                    if (!add.value.trim()) return
                    list.push({id: add.value.trim(), level: '1', auto: '0'})
                    out()
                })
                add.placeholder = '+ id'
                add.dataset.list = 'abilities:add'
                sections.push(el('div', {className: 'thing-list'}, el('div', {className: 'thing-head', textContent: 'Abilities'}), ...rows, el('div', {className: 'thing-row'}, add)))
            } else if (field === 'drops') {
                const sets = text.split(';').filter(Boolean).map(set => set.split(',').filter(Boolean).map(e => {
                    const [id, chance = '100'] = e.split(':')
                    return {id, chance}
                }))
                const out = () => send('drops', sets.map(set => set.filter(e => e.id).map(e => `${e.id}:${e.chance}`).join(',')).filter(Boolean).join(';'))
                const blocks = sets.map((set, n) => {
                    const rows = set.map(e => {
                        const input = idInput(e.id, () => {
                            e.id = input.value.trim()
                            out()
                        })
                        const chance = el('input', {type: 'number', min: 0, max: 100, value: e.chance, className: 'thing-number', title: 'Chance (%)'})
                        chance.onchange = () => {
                            e.chance = chance.value
                            out()
                        }
                        const remove = el('button', {textContent: '×', title: 'Take it out'})
                        remove.onclick = () => {
                            set.splice(set.indexOf(e), 1)
                            out()
                        }
                        return el('div', {className: 'thing-row'}, input, named(ITEMS, e.id), chance, remove)
                    })
                    const add = idInput('', () => {
                        if (!add.value.trim()) return
                        set.push({id: add.value.trim(), chance: '100'})
                        out()
                    })
                    add.placeholder = '+ id'
                    add.dataset.list = `drops:${n}:add`
                    const total = set.reduce((t, e) => t + Number(e.chance || 0), 0)
                    return el('div', {className: 'thing-set'}, el('div', {className: 'thing-sub', textContent: `Set ${n + 1} · ${total}%`}), ...rows, el('div', {className: 'thing-row'}, add))
                })
                // A new set by its first item.
                const addSet = idInput('', () => {
                    if (!addSet.value.trim()) return
                    sets.push([{id: addSet.value.trim(), chance: '100'}])
                    out()
                })
                addSet.placeholder = '+ set: id'
                addSet.dataset.list = 'drops:new'
                sections.push(el('div', {className: 'thing-list'}, el('div', {className: 'thing-head', textContent: 'Item drops'}), ...blocks, el('div', {className: 'thing-row'}, addSet)))
            }
        }
        root.replaceChildren(...sections)
    }

    /** The kinds the lists name objects of: their names wanted. */
    const KINDS = [ITEMS, ABILITIES, UNITS]

    return {render, KINDS}
})()
