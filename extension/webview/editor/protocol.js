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
        RELOAD: 16,
        LIGHT: 145,
        MAP_ASK: 19,
        RESIZE: 20,
        TILES: 21,
        TILE_PATHING: 22,
        CAMERA: 23,
        CAMERA_VIEW: 24,
        CAMERA_RENAME: 25,
        CAMERA_DELETE: 26,
        CAMERA_SET: 27,
        THING_ASK: 28,
        THING_SET: 29,
        THING_SET_TEXT: 39,
        IMPORT_LIST: 40,
        IMPORT_ADD: 41,
        IMPORT_SET: 42,
        IMPORT_DELETE: 43,
        IMPORT_PREVIEW: 44,
        IMPORTS: 153,
        INFO_ASK: 45,
        INFO_SET: 46,
        INFO: 155,
        SOUND_FILES_ASK: 47,
        SOUND_FILES: 156,
        IMPORT_PREVIEW_ANSWER: 154,
        THING: 147,
        SOUND_NEW: 30,
        SOUND_SET: 31,
        SOUND_DELETE: 32,
        SOUND_PLAY: 33,
        SOUNDS: 148,
        SOUND_DATA: 149,
        OBJECT_LIST: 34,
        OBJECT_ASK: 35,
        OBJECT_SET: 36,
        OBJECT_NEW: 37,
        OBJECT_DELETE: 38,
        OBJECT_LIST_ANSWER: 150,
        OBJECT_FIELDS: 151,
        OBJECT_ENUMS: 152,
        MAP_SETTINGS: 146,
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
        reload: () => new Writer().frame(KIND.RELOAD),
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
        /** A camera's target dragged: the drag's first step, its index, the ray where it is. */
        camera(start, index, {origin, dir}) {
            const w = new Writer()
            w.u8(start ? 1 : 0)
            w.u32(index)
            for (const v of [...origin, ...dir]) w.f32(v)
            return w.frame(KIND.CAMERA)
        },
        /** A camera made from the view (index -1) or set to it: the point looked at (cells), distance (cells), horizontal, vertical (radians), field of view (degrees), roll (radians). */
        cameraView(index, position, distance, horizontal, vertical, fov, roll) {
            const w = new Writer()
            w.u32(index >>> 0)
            for (const v of [...position, distance, horizontal, vertical, fov, roll]) w.f32(v)
            return w.frame(KIND.CAMERA_VIEW)
        },
        cameraRename(index, name) {
            const w = new Writer()
            w.u32(index)
            w.string(name)
            return w.frame(KIND.CAMERA_RENAME)
        },
        cameraDelete(index) {
            const w = new Writer()
            w.u32(index)
            return w.frame(KIND.CAMERA_DELETE)
        },
        /** A camera's field (`field_of_view` ...) set. */
        cameraSet(index, field, value) {
            const w = new Writer()
            w.u32(index)
            w.string(field)
            w.f32(value)
            return w.frame(KIND.CAMERA_SET)
        },
        /** The fields of thing `index` of `file` asked for: answered by `thing`. */
        thingAsk(file, index) {
            const w = new Writer()
            w.u8(file)
            w.u32(index)
            return w.frame(KIND.THING_ASK)
        },
        /** A thing's field (`facing`, `scale_x`, `player` ...) set. */
        thingSet(file, index, field, value) {
            const w = new Writer()
            w.u8(file)
            w.u32(index)
            w.string(field)
            w.f32(value)
            return w.frame(KIND.THING_SET)
        },
        /** A thing's list (`items`, `abilities`, `drops`) set from its text. */
        thingSetText(file, index, field, value) {
            const w = new Writer()
            w.u8(file)
            w.u32(index)
            w.string(field)
            w.string(value)
            return w.frame(KIND.THING_SET_TEXT)
        },
        infoAsk: () => new Writer().frame(KIND.INFO_ASK),
        soundFiles: () => new Writer().frame(KIND.SOUND_FILES_ASK),
        /** A field of the map's info (`name`, `player.0.race` ...) set from its text. */
        infoSet(field, value) {
            const w = new Writer()
            w.string(field)
            w.string(value)
            return w.frame(KIND.INFO_SET)
        },
        importList: () => new Writer().frame(KIND.IMPORT_LIST),
        /** A file on disk imported at `path` ('' war3mapImported\ and its name). */
        importAdd(source, path) {
            const w = new Writer()
            w.string(source)
            w.string(path)
            return w.frame(KIND.IMPORT_ADD)
        },
        importSet(index, path) {
            const w = new Writer()
            w.u32(index)
            w.string(path)
            return w.frame(KIND.IMPORT_SET)
        },
        importDelete(index) {
            const w = new Writer()
            w.u32(index)
            return w.frame(KIND.IMPORT_DELETE)
        },
        importPreview(index) {
            const w = new Writer()
            w.u32(index)
            return w.frame(KIND.IMPORT_PREVIEW)
        },
        /** A sound of `file` made. */
        soundNew(file) {
            const w = new Writer()
            w.string(file)
            return w.frame(KIND.SOUND_NEW)
        },
        /** A sound's field (`name`, `volume` ...) set to `value`, text ('' "use default"). */
        soundSet(index, field, value) {
            const w = new Writer()
            w.u32(index)
            w.string(field)
            w.string(String(value))
            return w.frame(KIND.SOUND_SET)
        },
        soundDelete(index) {
            const w = new Writer()
            w.u32(index)
            return w.frame(KIND.SOUND_DELETE)
        },
        /** A sound file's bytes asked for: answered by `soundData`. */
        soundPlay(file) {
            const w = new Writer()
            w.string(file)
            return w.frame(KIND.SOUND_PLAY)
        },
        /** Kind `k`'s objects asked for (`EditorObjectEditor.KINDS`' order). */
        objectList(k) {
            const w = new Writer()
            w.u8(k)
            return w.frame(KIND.OBJECT_LIST)
        },
        objectAsk(k, id) {
            const w = new Writer()
            w.u8(k)
            w.string(id)
            return w.frame(KIND.OBJECT_ASK)
        },
        /** An object's field set: its value as text ('' the game's again). */
        objectSet(k, id, field, level, column, value) {
            const w = new Writer()
            w.u8(k)
            w.string(id)
            w.string(field)
            w.u32(level)
            w.u32(column)
            w.string(String(value))
            return w.frame(KIND.OBJECT_SET)
        },
        objectNew(k, base) {
            const w = new Writer()
            w.u8(k)
            w.string(base)
            return w.frame(KIND.OBJECT_NEW)
        },
        objectDelete(k, id) {
            const w = new Writer()
            w.u8(k)
            w.string(id)
            return w.frame(KIND.OBJECT_DELETE)
        },
        mapAsk: () => new Writer().frame(KIND.MAP_ASK),
        /** Cells added on each side {left, right, bottom, top}, the playable area's margins [l, r, b, t] or null. */
        resize(sides, margins) {
            const w = new Writer()
            for (const v of [sides.left, sides.right, sides.bottom, sides.top]) w.u32(v | 0)
            w.u8(margins ? 1 : 0)
            for (const v of margins ?? [0, 0, 0, 0]) w.u32(v | 0)
            return w.frame(KIND.RESIZE)
        },
        /** The tile setter's: the tiles' ids, each old tile's new index, the tileset's letter. */
        tiles(ids, to, tileset) {
            const w = new Writer()
            w.u32(ids.length)
            for (const id of ids) w.string(id)
            w.u32(to.length)
            for (const v of to) w.u8(v)
            w.u8(tileset.charCodeAt(0))
            return w.frame(KIND.TILES)
        },
        tilePathing(index, forbids) {
            const w = new Writer()
            w.u8(index)
            w.u8(forbids)
            return w.frame(KIND.TILE_PATHING)
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
                        layers: r.list(r => ({texture: r.i32(), filterMode: r.u32(), shading: r.u32(), alpha: r.f32(), hd: r.u8() !== 0, normal: r.i32(), orm: r.i32(), emissive: r.i32(), team: r.i32(), reflections: r.i32()})),
                        skin: r.bytes(),
                        index: r.u32(),
                        tangents: r.f32s(),
                    })),
                    instances: r.f32s(),
                    error: r.string(),
                    anim: {bones: r.u32(), geosets: r.u32(), frames: r.u32(), fps: r.f32(), matrices: r.f32s()},
                    emitters: r.list(r => ({params: r.f32s(), segments: r.f32s(), head: r.f32s(), tail: r.f32s(), texture: r.i32(), fps: r.f32(), frames: r.f32s()})),
                    ribbons: r.list(r => ({params: r.f32s(), color: r.f32s(), texture: r.i32(), fps: r.f32(), frames: r.f32s()})),
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
            case KIND.OBJECT_LIST_ANSWER:
                return {kind: 'objectList', objectKind: r.u8(), objects: r.list(r => ({id: r.string(), name: r.string(), group: r.string(), state: r.u8()}))}
            case KIND.OBJECT_FIELDS: {
                const m = {kind: 'objectFields', objectKind: r.u8(), id: r.string(), base: r.string(), name: r.string(), levels: r.u32()}
                m.fields = r.list(r => ({field: r.string(), name: r.string(), category: r.string(), ty: r.string(), level: r.u32(), column: r.u32(), value: r.string(), default: r.string(), changed: r.u8() !== 0, hint: r.string()}))
                return m
            }
            case KIND.OBJECT_ENUMS:
                return {kind: 'objectEnums', enums: r.list(r => ({ty: r.string(), multi: r.u8() !== 0, values: r.list(r => [r.string(), r.string()])}))}
            case KIND.SOUND_FILES:
                return {kind: 'soundFiles', files: r.list(r => r.string())}
            case KIND.INFO:
                return {kind: 'info', fields: r.list(r => [r.string(), r.string()])}
            case KIND.IMPORTS:
                return {kind: 'imports', files: r.list(r => ({path: r.string(), kept: r.string(), flags: r.u8(), found: r.u8() !== 0, pending: r.u8() !== 0}))}
            case KIND.IMPORT_PREVIEW_ANSWER: {
                const m = {kind: 'importPreview', path: r.string(), what: r.u8(), size: r.u32()}
                m.picture = {width: r.u32(), height: r.u32(), rgba: r.bytes()}
                m.error = r.string()
                return m
            }
            case KIND.SOUNDS:
                // `numbers` as `w3s_rs::Sound::numbers` has them.
                return {kind: 'sounds', sounds: r.list(r => ({name: r.string(), file: r.string(), eax: r.string(), flags: r.u32(), numbers: Array.from(r.f32s())}))}
            case KIND.SOUND_DATA:
                return {kind: 'soundData', file: r.string(), bytes: r.bytes(), error: r.string()}
            case KIND.THING: {
                const file = r.u8()
                const index = r.u32()
                const fields = r.list(r => [r.string(), r.f32()])
                return {kind: 'thing', file, index, fields, texts: r.list(r => [r.string(), r.string()])}
            }
            case KIND.OVERLAYS: {
                const grid = r => ({width: r.u32(), height: r.u32(), data: r.bytes()})
                return {
                    kind: 'overlays',
                    pathing: grid(r),
                    destructables: grid(r),
                    regionPicture: grid(r),
                    regions: r.list(r => ({name: r.string(), bounds: [r.f32(), r.f32(), r.f32(), r.f32()], color: [r.u8(), r.u8(), r.u8()]})),
                    bounds: r.f32s(),
                    cameraLines: r.f32s(),
                    cameras: r.list(r => ({name: r.string(), target: [r.f32(), r.f32(), r.f32()], eye: [r.f32(), r.f32(), r.f32()], view: [r.f32(), r.f32(), r.f32(), r.f32(), r.f32(), r.f32()]})),
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
            case KIND.MAP_SETTINGS: {
                const width = r.u32()
                const height = r.u32()
                const info = r.u8() !== 0
                const margins = [r.i32(), r.i32(), r.i32(), r.i32()]
                const tileset = r.string()
                const maxTiles = r.u32()
                const tilesets = r.list(r => ({letter: r.string(), name: r.string()}))
                const mapTiles = r.list(r => r.string())
                const tiles = r.list(r => ({id: r.string(), name: r.string(), cliff: r.u8() !== 0, pathing: r.u8()}))
                return {kind: 'mapSettings', width, height, info, margins, tileset, maxTiles, tilesets, mapTiles, tiles}
            }
            case KIND.LIGHT: {
                const terrain = r.f32s()
                const units = r.f32s()
                const shadows = {width: r.u32(), height: r.u32(), data: r.bytes()}
                const on = r.u8() !== 0
                const fog = {style: r.i32(), start: r.f32(), end: r.f32(), density: r.f32(), color: [r.u8(), r.u8(), r.u8(), r.u8()]}
                return {kind: 'light', terrain, units, shadows, fog: on ? fog : null, notes: r.list(r => r.string())}
            }
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
