'use strict'

/**
 * The editor protocol, the page's side: the twin of
 * `editor/src/protocol.rs`, which documents it; keep the two in step. The
 * extension hands the page each frame of the editor's process as its kind
 * and body, and sends the page's frames to it as they are.
 */
const EditorProtocol = (() => {
    const KIND = {
        HELLO: 1,
        MAP: 128,
        ERROR: 129,
        TERRAIN: 130,
        GROUND: 131,
        WATER: 132,
        CLIFFS: 133,
        OBJECTS: 134,
        PICKED: 135,
        OVERLAYS: 136,
        PICK: 2,
        BRUSH: 3,
        STROKE_END: 4,
        UNDO: 5,
        REDO: 6,
        SAVE: 7,
        PALETTE: 137,
        STATE: 138,
        PLACE: 8,
        MOVE: 9,
        ROTATE: 10,
        DELETE: 11,
        INSTANCES: 139,
        OBJECT_PALETTE: 140,
        ICONS: 12,
        ICONS_ANSWER: 141,
        REGION: 13,
        REGION_RENAME: 14,
        REGION_DELETE: 15,
    }

    /** The tools by their code (`edit::TOOLS`). */
    const TOOLS = ['raise', 'lower', 'plateau', 'smooth', 'texture', 'cliffUp', 'cliffDown', 'water', 'dry', 'pathingOn', 'pathingOff']

    const utf8 = new TextDecoder()
    const utf8Out = new TextEncoder()

    class Reader {
        /** @param {Uint8Array} body */
        constructor(body) {
            this.body = body
            this.view = new DataView(body.buffer, body.byteOffset, body.byteLength)
            this.at = 0
        }

        need(n) {
            if (this.at + n > this.body.length) throw new Error(`malformed message: ${n} bytes wanted, ${this.body.length - this.at} left`)
        }

        u8() {
            this.need(1)
            return this.body[this.at++]
        }

        u32() {
            this.need(4)
            const v = this.view.getUint32(this.at, true)
            this.at += 4
            return v
        }

        i32() {
            this.need(4)
            const v = this.view.getInt32(this.at, true)
            this.at += 4
            return v
        }

        f32() {
            this.need(4)
            const v = this.view.getFloat32(this.at, true)
            this.at += 4
            return v
        }

        /** An array's bytes, copied so a typed array can view them. */
        bytes() {
            const n = this.u32()
            this.need(n)
            const b = this.body.slice(this.at, this.at + n)
            this.at += n
            return b
        }

        f32s() {
            return new Float32Array(this.bytes().buffer)
        }

        u16s() {
            return new Uint16Array(this.bytes().buffer)
        }

        u32s() {
            return new Uint32Array(this.bytes().buffer)
        }

        string() {
            const n = this.u32()
            this.need(n)
            const s = utf8.decode(this.body.subarray(this.at, this.at + n))
            this.at += n
            return s
        }

        list(each) {
            const n = this.u32()
            const out = []
            for (let i = 0; i < n; i++) out.push(each(this))
            return out
        }
    }

    class Writer {
        constructor() {
            /** @type {number[]} */
            this.bytes = []
        }

        u8(v) {
            this.bytes.push(v & 0xff)
        }

        u32(v) {
            this.bytes.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff)
        }

        f32(v) {
            const b = new Uint8Array(new Float32Array([v]).buffer)
            for (const x of b) this.bytes.push(x)
        }

        string(s) {
            const b = utf8Out.encode(s)
            this.u32(b.length)
            for (const x of b) this.bytes.push(x)
        }

        /** The whole frame. */
        frame(kind) {
            const out = new Uint8Array(5 + this.bytes.length)
            new DataView(out.buffer).setUint32(0, this.bytes.length, true)
            out[4] = kind
            out.set(this.bytes, 5)
            return out
        }
    }

    const encode = {
        hello: () => new Writer().frame(KIND.HELLO),
        /** A ray from the camera: origin and direction (cells, layers). */
        pick(origin, dir) {
            const w = new Writer()
            for (const v of [...origin, ...dir]) w.f32(v)
            return w.frame(KIND.PICK)
        },
        /** A step of a stroke: `brush` {tool, size, strength, texture, cliff}. */
        brush(start, origin, dir, brush) {
            const w = new Writer()
            w.u8(start ? 1 : 0)
            w.u8(TOOLS.indexOf(brush.tool))
            for (const v of [...origin, ...dir, brush.size, brush.strength]) w.f32(v)
            w.string(brush.texture ?? '')
            w.u32(brush.cliff ?? -1)
            w.u8(brush.flags ?? 0)
            return w.frame(KIND.BRUSH)
        },
        strokeEnd: () => new Writer().frame(KIND.STROKE_END),
        undo: () => new Writer().frame(KIND.UNDO),
        redo: () => new Writer().frame(KIND.REDO),
        save: () => new Writer().frame(KIND.SAVE),
        /** A thing placed: kind (0 doodad, 1 destructable, 2 unit, 3 item), type id, player, the ray. */
        place(kind, id, player, origin, dir) {
            const w = new Writer()
            w.u8(kind)
            w.string(id)
            w.u32(player)
            for (const v of [...origin, ...dir]) w.f32(v)
            return w.frame(KIND.PLACE)
        },
        /** Thing `index` of `file` moved where the ray meets the ground; `start` a drag's first. */
        move(start, file, index, origin, dir) {
            const w = new Writer()
            w.u8(start ? 1 : 0)
            w.u8(file)
            w.u32(index)
            for (const v of [...origin, ...dir]) w.f32(v)
            return w.frame(KIND.MOVE)
        },
        rotate(file, index, delta) {
            const w = new Writer()
            w.u8(file)
            w.u32(index)
            w.f32(delta)
            return w.frame(KIND.ROTATE)
        },
        /** The icons of types: a list of [kind, id]. */
        icons(list) {
            const w = new Writer()
            w.u32(list.length)
            for (const [k, id] of list) {
                w.u8(k)
                w.string(id)
            }
            return w.frame(KIND.ICONS)
        },
        /** A region's drag: its first step, mode (0 new, 1 moved, 2 a corner), index (-1 new), the rays it started and is at. */
        region(start, mode, index, from, to) {
            const w = new Writer()
            w.u8(start ? 1 : 0)
            w.u8(mode)
            w.u32(index)
            for (const v of [...from.origin, ...from.dir, ...to.origin, ...to.dir]) w.f32(v)
            return w.frame(KIND.REGION)
        },
        regionRename(index, name) {
            const w = new Writer()
            w.u32(index)
            w.string(name)
            return w.frame(KIND.REGION_RENAME)
        },
        regionDelete(index) {
            const w = new Writer()
            w.u32(index)
            return w.frame(KIND.REGION_DELETE)
        },
        delete(file, index) {
            const w = new Writer()
            w.u8(file)
            w.u32(index)
            return w.frame(KIND.DELETE)
        },
    }

    /**
     * A frame from the editor as a message, or `null` for a kind this page
     * doesn't know.
     *
     * @param {number} kind
     * @param {Uint8Array} body
     */
    function decode(kind, body) {
        const r = new Reader(body)
        switch (kind) {
            case KIND.MAP:
                return {
                    kind: 'map',
                    project: r.string(),
                    name: r.string(),
                    author: r.string(),
                    width: r.u32(),
                    height: r.u32(),
                    tileset: r.string(),
                    tilesetName: r.string(),
                    notes: r.list(r => r.string()),
                }
            case KIND.ERROR:
                return {kind: 'error', message: r.string()}
            case KIND.TERRAIN:
                return {
                    kind: 'terrain',
                    width: r.u32(),
                    height: r.u32(),
                    offset: [r.f32(), r.f32()],
                    heights: r.f32s(),
                    ground: r.f32s(),
                    water: r.f32s(),
                    corners: r.bytes(),
                    tiles: r.u32s(),
                    tileFlags: r.bytes(),
                }
            case KIND.GROUND: {
                const size = r.u32()
                const count = r.u32()
                const textures = r.list(r => ({id: r.string(), path: r.string(), first: r.u32(), count: r.u32(), extended: r.u8() !== 0, error: r.string()}))
                return {kind: 'ground', size, count, textures, rgba: r.bytes()}
            }
            case KIND.WATER: {
                const rate = r.f32()
                const colors = [0, 1, 2, 3].map(() => [r.u8(), r.u8(), r.u8(), r.u8()])
                const size = r.u32()
                const count = r.u32()
                return {kind: 'water', rate, colors, size, count, rgba: r.bytes(), error: r.string()}
            }
            case KIND.CLIFFS: {
                const size = r.u32()
                const count = r.u32()
                const rgba = r.bytes()
                const textures = r.list(r => ({path: r.string(), error: r.string()}))
                const meshes = r.list(r => ({
                    path: r.string(),
                    positions: r.f32s(),
                    uvs: r.f32s(),
                    normals: r.f32s(),
                    indices: r.u16s(),
                    instances: r.f32s(),
                    error: r.string(),
                }))
                return {kind: 'cliffs', size, count, rgba, textures, meshes, notes: r.list(r => r.string())}
            }
            case KIND.OBJECTS: {
                const firstTexture = r.u32()
                const firstModel = r.u32()
                const textures = r.list(r => ({path: r.string(), width: r.u32(), height: r.u32(), rgba: r.bytes(), error: r.string()}))
                const models = r.list(r => ({
                    path: r.string(),
                    min: [r.f32(), r.f32(), r.f32()],
                    max: [r.f32(), r.f32(), r.f32()],
                    geosets: r.list(r => ({
                        positions: r.f32s(),
                        normals: r.f32s(),
                        uvs: r.f32s(),
                        indices: r.u16s(),
                        layers: r.list(r => ({texture: r.i32(), filterMode: r.u32(), shading: r.u32(), alpha: r.f32()})),
                    })),
                    instances: r.f32s(),
                    error: r.string(),
                }))
                return {kind: 'objects', firstTexture, firstModel, textures, models, notes: r.list(r => r.string())}
            }
            case KIND.PICKED: {
                const m = {kind: 'picked', found: r.u8() !== 0, model: r.u32(), instance: r.u32(), file: r.u8(), index: r.u32()}
                const region = r.u32()
                m.region = region === 0xFFFFFFFF ? null : region
                m.text = r.string()
                const on = r.u8() !== 0
                const p = [r.f32(), r.f32(), r.f32()]
                return {...m, ground: on ? p : null}
            }
            case KIND.OVERLAYS: {
                const grid = r => ({width: r.u32(), height: r.u32(), data: r.bytes()})
                return {
                    kind: 'overlays',
                    pathing: grid(r),
                    regionPicture: grid(r),
                    regions: r.list(r => ({name: r.string(), bounds: [r.f32(), r.f32(), r.f32(), r.f32()], color: [r.u8(), r.u8(), r.u8()]})),
                    bounds: r.f32s(),
                    cameraLines: r.f32s(),
                    cameras: r.list(r => ({name: r.string(), target: [r.f32(), r.f32(), r.f32()], eye: [r.f32(), r.f32(), r.f32()]})),
                    minimap: grid(r),
                    notes: r.list(r => r.string()),
                }
            }
            case KIND.PALETTE: {
                const entry = r => ({id: r.string(), name: r.string(), icon: {width: r.u32(), height: r.u32(), rgba: r.bytes()}})
                return {kind: 'palette', grounds: r.list(entry), cliffs: r.list(entry)}
            }
            case KIND.INSTANCES:
                return {kind: 'instances', models: r.list(r => r.f32s())}
            case KIND.OBJECT_PALETTE:
                return {kind: 'objectPalette', entries: r.list(r => ({kind: r.u8(), id: r.string(), name: r.string(), group: r.string(), custom: r.u8() !== 0}))}
            case KIND.ICONS_ANSWER:
                return {kind: 'icons', icons: r.list(r => ({kind: r.u8(), id: r.string(), width: r.u32(), height: r.u32(), rgba: r.bytes()}))}
            case KIND.STATE:
                return {kind: 'state', dirty: r.u8() !== 0, undo: r.u8() !== 0, redo: r.u8() !== 0, message: r.string()}
            default:
                return null
        }
    }

    return {KIND, TOOLS, encode, decode}
})()
