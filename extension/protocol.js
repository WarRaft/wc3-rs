'use strict'

/**
 * The binary wire protocol between the extension and `wc3-server`; the
 * Rust twin is `server/src/protocol.rs`, which documents the format. Keep
 * the two in step.
 *
 * Every message is one frame: `u32 body length (LE) | u8 kind | body`.
 * Fields are little-endian; a string is a `u32` byte length plus UTF-8; a
 * list is a `u32` count plus its items; a range is four `u32` (start line,
 * start character, end line, end character).
 *
 * A document is identified by an id: its path on disk for a file
 * (`uri.fsPath`), its URI otherwise. The server reports project files the
 * editor never opened, and a path is what both sides know them by.
 */

const HEADER = 5

const KIND = {
    INIT: 1,
    OPEN: 2,
    EDIT: 3,
    LANG: 4,
    CLOSE: 5,
    QUERY: 6,
    FS: 7,
    TEXTURE: 8,
    OBJECTS: 9,
    FILE: 128,
    LEGEND: 129,
    ANSWER: 130,
    FS_ANSWER: 131,
    TEXTURE_ANSWER: 132,
    OBJECTS_ANSWER: 133,
}

/** What `encode.objects` asks for (`objects::TO_TEXT` / `TO_BYTES`). */
const OBJECTS_OP = {
    TO_TEXT: 0,
    TO_BYTES: 1,
}

/** File system operations inside maps (`protocol::FsOp` on the Rust side). */
const FS_OP = {
    STAT: 0,
    READ_DIR: 1,
    READ: 2,
    WRITE: 3,
    DELETE: 4,
    RENAME: 5,
    CREATE_DIR: 6,
}

/** Flags of a file system operation. */
const FS_FLAG = {
    CREATE: 1,
    OVERWRITE: 2,
    RECURSIVE: 4,
}

/** File system errors, by wire code. */
const FS_ERRORS = [null, 'FileNotFound', 'FileExists', 'FileNotADirectory', 'FileIsADirectory', 'NoPermissions', 'Unavailable']

/** Query kinds (`index::QueryKind` on the Rust side). */
const QUERY = {
    DEFINITION: 0,
    REFERENCES: 1,
    HOVER: 2,
    DOCUMENT_SYMBOLS: 3,
    WORKSPACE_SYMBOLS: 4,
    PREPARE_RENAME: 5,
    RENAME: 6,
    CONTENT: 7,
    PATH_COMPLETION: 8,
    PATH_LINKS: 9,
    SLK_TABLE: 10,
    SLK_SET_CELL: 11,
    INLAY_HINTS: 12,
    COLORS: 13,
    TXT2_HTML: 14,
    TRIGGER_VIEW: 15,
    TRIGGER_EDIT: 16,
}

/** Symbol kinds, in wire order (`parser_rs::SymbolKind`). */
const SYMBOL_KINDS = [
    'function', 'native', 'global', 'constant', 'local', 'parameter', 'type',
    'library', 'struct', 'interface', 'module', 'method', 'field',
    'namespace', 'class', 'enum', 'enum member',
]

class Writer {
    constructor() {
        this.buf = Buffer.allocUnsafe(256)
        this.len = HEADER
    }

    /** @param {number} n */
    ensure(n) {
        if (this.len + n <= this.buf.length) return
        const grown = Buffer.allocUnsafe(Math.max(this.buf.length * 2, this.len + n))
        this.buf.copy(grown, 0, 0, this.len)
        this.buf = grown
    }

    /** @param {number} v */
    u8(v) {
        this.ensure(1)
        this.buf.writeUInt8(v, this.len++)
    }

    /** @param {number} v */
    u32(v) {
        this.ensure(4)
        this.buf.writeUInt32LE(v >>> 0, this.len)
        this.len += 4
    }

    /** @param {number} v */
    i32(v) {
        this.ensure(4)
        this.buf.writeInt32LE(v | 0, this.len)
        this.len += 4
    }

    /** @param {string} s */
    string(s) {
        const n = Buffer.byteLength(s, 'utf8')
        this.u32(n)
        this.ensure(n)
        this.buf.write(s, this.len, n, 'utf8')
        this.len += n
    }

    /** @param {Uint8Array} bytes */
    bytes(bytes) {
        this.u32(bytes.length)
        this.ensure(bytes.length)
        this.buf.set(bytes, this.len)
        this.len += bytes.length
    }

    /** @param {import('vscode').Range} r */
    range(r) {
        this.u32(r.start.line)
        this.u32(r.start.character)
        this.u32(r.end.line)
        this.u32(r.end.character)
    }

    /**
     * Fills in the header and returns the whole frame.
     * @param {number} kind
     */
    frame(kind) {
        this.buf.writeUInt32LE(this.len - HEADER, 0)
        this.buf.writeUInt8(kind, 4)
        return this.buf.subarray(0, this.len)
    }
}

/** Encoders for editor → server messages; each returns a whole frame. */
const encode = {
    /** @param {string[]} roots */
    init(roots) {
        const w = new Writer()
        w.u32(roots.length)
        for (const root of roots) w.string(root)
        return w.frame(KIND.INIT)
    },

    /**
     * @param {string} id
     * @param {string} path  empty when the document has no file
     * @param {string} lang
     * @param {number} version
     * @param {string} text
     */
    open(id, path, lang, version, text) {
        const w = new Writer()
        w.string(id)
        w.string(path)
        w.string(lang)
        w.i32(version)
        w.string(text)
        return w.frame(KIND.OPEN)
    },

    /**
     * @param {string} id
     * @param {number} version
     * @param {readonly import('vscode').TextDocumentContentChangeEvent[]} changes
     */
    edit(id, version, changes) {
        const w = new Writer()
        w.string(id)
        w.i32(version)
        w.u32(changes.length)
        for (const change of changes) {
            w.range(change.range)
            w.string(change.text)
        }
        return w.frame(KIND.EDIT)
    },

    /** @param {string} id @param {string} lang */
    lang(id, lang) {
        const w = new Writer()
        w.string(id)
        w.string(lang)
        return w.frame(KIND.LANG)
    },

    /** @param {string} id */
    close(id) {
        const w = new Writer()
        w.string(id)
        return w.frame(KIND.CLOSE)
    },

    /**
     * @param {number} request  echoed in the answer
     * @param {number} kind  one of {@link QUERY}
     * @param {string} id
     * @param {{line: number, character: number}} pos
     * @param {string} text  the search text (workspace symbols)
     */
    query(request, kind, id, pos, text) {
        const w = new Writer()
        w.u32(request)
        w.u8(kind)
        w.string(id)
        w.u32(pos.line)
        w.u32(pos.character)
        w.string(text)
        return w.frame(KIND.QUERY)
    },

    /**
     * @param {number} request  echoed in the answer
     * @param {number} op  one of {@link FS_OP}
     * @param {string} path
     * @param {string} to  a rename's target, empty otherwise
     * @param {number} flags  {@link FS_FLAG}s
     * @param {Uint8Array} data  a write's bytes
     */
    fs(request, op, path, to, flags, data) {
        const w = new Writer()
        w.u32(request)
        w.u8(op)
        w.string(path)
        w.string(to)
        w.u8(flags)
        w.bytes(data)
        return w.frame(KIND.FS)
    },

    /**
     * @param {number} request  echoed in the answer
     * @param {string} name  the file's path: what it is, and where what it
     *   needs is (a shadow map's terrain)
     * @param {Uint8Array} data  the file's bytes
     */
    texture(request, name, data) {
        const w = new Writer()
        w.u32(request)
        w.string(name)
        w.bytes(data)
        return w.frame(KIND.TEXTURE)
    },

    /**
     * Object data to convert: a `.w3a` ... file's bytes to its text form,
     * or a text form (UTF-8) to the file's bytes.
     *
     * @param {number} request  echoed in the answer
     * @param {number} op  one of OBJECTS_OP
     * @param {string} name  the file's name or path; its extension says what it holds
     * @param {Uint8Array} data
     */
    objects(request, op, name, data) {
        const w = new Writer()
        w.u32(request)
        w.u8(op)
        w.string(name)
        w.bytes(data)
        return w.frame(KIND.OBJECTS)
    },
}

/** Reads the fields of one frame's body in order. */
class Reader {
    /** @param {Buffer} body */
    constructor(body) {
        this.body = body
        this.at = 0
    }

    u8() {
        return this.body.readUInt8(this.at++)
    }

    u32() {
        const v = this.body.readUInt32LE(this.at)
        this.at += 4
        return v
    }

    i32() {
        const v = this.body.readInt32LE(this.at)
        this.at += 4
        return v
    }

    u64() {
        const v = Number(this.body.readBigUInt64LE(this.at))
        this.at += 8
        return v
    }

    /** A copy of the next bytes field. */
    bytes() {
        const n = this.u32()
        const out = new Uint8Array(n)
        out.set(this.body.subarray(this.at, this.at + n))
        this.at += n
        return out
    }

    string() {
        const n = this.u32()
        const s = this.body.toString('utf8', this.at, this.at + n)
        this.at += n
        return s
    }

    strings() {
        const out = []
        for (let n = this.u32(); n > 0; n--) out.push(this.string())
        return out
    }

    /** @returns {[number, number, number, number]} */
    range() {
        return [this.u32(), this.u32(), this.u32(), this.u32()]
    }

    /**
     * A list of `u32`, copied into its own (aligned) array in one go.
     * Every platform VS Code runs on is little-endian, like the wire.
     */
    u32s() {
        const n = this.u32()
        const out = new Uint32Array(n)
        new Uint8Array(out.buffer).set(this.body.subarray(this.at, this.at + n * 4))
        this.at += n * 4
        return out
    }
}

/**
 * @typedef {{severity: number, range: [number, number, number, number], message: string, rule: string, fix: WireFix | null}} WireDiagnostic
 * @typedef {{title: string, edits: {range: [number, number, number, number], text: string}[]}} WireFix
 * @typedef {{id: string, range: [number, number, number, number], name: string, container: string, kind: string}} WireLocation
 * @typedef {{kind: 'file', id: string, version: number, diagnostics: WireDiagnostic[], tokens: Uint32Array}
 *         | {kind: 'legend', types: string[], modifiers: string[]}
 *         | {kind: 'answer', request: number, text: string, locations: WireLocation[]}
 *         | {kind: 'fs', request: number, error: string | null, message: string, stat: FsStat | null, entries: [string, 'file' | 'dir'][], data: Uint8Array}
 *         | {kind: 'texture', request: number, texture: Texture}
 *         | {kind: 'objects', request: number, error: string, data: Uint8Array}} ServerMessage
 * @typedef {{error: string, version: string, encoding: string, alphaBits: number, width: number, height: number, mips: TextureMip[], cells: Uint8Array, flags: {bit: number, name: string}[]}} Texture
 * @typedef {{width: number, height: number, error: string, rgba: Uint8Array}} TextureMip
 * @typedef {{type: 'file' | 'dir', size: number, mtime: number, readonly: boolean, note: string}} FsStat
 */

/**
 * Decodes a server → editor frame; `null` for a kind this extension
 * doesn't know, so the server can add messages.
 *
 * @param {number} kind
 * @param {Buffer} body
 * @returns {ServerMessage | null}
 */
function decode(kind, body) {
    const r = new Reader(body)
    switch (kind) {
        case KIND.FILE: {
            const id = r.string()
            const version = r.i32()
            const diagnostics = []
            for (let n = r.u32(); n > 0; n--) {
                const d = {severity: r.u8(), range: r.range(), message: r.string(), rule: r.string(), fix: null}
                const title = r.string()
                const edits = []
                for (let e = r.u32(); e > 0; e--) edits.push({range: r.range(), text: r.string()})
                if (title) d.fix = {title, edits}
                diagnostics.push(d)
            }
            return {kind: 'file', id, version, diagnostics, tokens: r.u32s()}
        }
        case KIND.LEGEND:
            return {kind: 'legend', types: r.strings(), modifiers: r.strings()}
        case KIND.ANSWER: {
            const request = r.u32()
            const text = r.string()
            const locations = []
            for (let n = r.u32(); n > 0; n--) {
                locations.push({id: r.string(), range: r.range(), name: r.string(), container: r.string(), kind: SYMBOL_KINDS[r.u8()]})
            }
            return {kind: 'answer', request, text, locations}
        }
        case KIND.FS_ANSWER: {
            const request = r.u32()
            const code = r.u8()
            const error = code ? FS_ERRORS[code] ?? 'Unavailable' : null
            const message = r.string()
            const type = r.u8()
            const size = r.u64()
            const mtime = r.u64()
            const readonly = r.u8() !== 0
            const note = r.string()
            const stat = type ? {type: type === 2 ? 'dir' : 'file', size, mtime, readonly, note} : null
            const entries = []
            for (let n = r.u32(); n > 0; n--) entries.push([r.string(), r.u8() === 2 ? 'dir' : 'file'])
            return {kind: 'fs', request, error, message, stat, entries, data: r.bytes()}
        }
        case KIND.TEXTURE_ANSWER: {
            const request = r.u32()
            const texture = {error: r.string(), version: r.string(), encoding: r.string(), alphaBits: r.u8(), width: r.u32(), height: r.u32(), mips: []}
            for (let n = r.u32(); n > 0; n--) {
                texture.mips.push({width: r.u32(), height: r.u32(), error: r.string(), rgba: r.bytes()})
            }
            // A pathing map's: each pixel's flags, and the flags' names.
            texture.cells = r.bytes()
            texture.flags = []
            for (let n = r.u32(); n > 0; n--) texture.flags.push({bit: r.u8(), name: r.string()})
            return {kind: 'texture', request, texture}
        }
        case KIND.OBJECTS_ANSWER:
            return {kind: 'objects', request: r.u32(), error: r.string(), data: r.bytes()}
        default:
            return null
    }
}

/**
 * Splits the server's output stream into frames. Chunks are joined only
 * once a whole frame has arrived, so a large frame isn't copied per chunk.
 */
class FrameReader {
    constructor() {
        /** @type {Buffer[]} */
        this.chunks = []
        this.size = 0
        /** Bytes needed for the next step: a header, then a whole frame. */
        this.need = HEADER
        this.bodyLength = -1
    }

    /**
     * @param {Buffer} chunk
     * @returns {{kind: number, body: Buffer}[]}
     */
    push(chunk) {
        this.chunks.push(chunk)
        this.size += chunk.length
        const frames = []
        while (this.size >= this.need) {
            const buf = this.chunks.length === 1 ? this.chunks[0] : Buffer.concat(this.chunks, this.size)
            if (this.bodyLength < 0) {
                this.chunks = [buf]
                this.bodyLength = buf.readUInt32LE(0)
                this.need = HEADER + this.bodyLength
                continue
            }
            frames.push({kind: buf[4], body: buf.subarray(HEADER, this.need)})
            const rest = buf.subarray(this.need)
            this.chunks = rest.length ? [rest] : []
            this.size = rest.length
            this.need = HEADER
            this.bodyLength = -1
        }
        return frames
    }
}

module.exports = {encode, decode, FrameReader, KIND, QUERY, FS_OP, FS_FLAG, OBJECTS_OP, HEADER}
