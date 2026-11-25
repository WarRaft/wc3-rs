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
    }

    const utf8 = new TextDecoder()
    const utf8Out = new TextEncoder()

    class Reader {
        /** @param {Uint8Array} body */
        constructor(body) {
            this.bytes = body
            this.view = new DataView(body.buffer, body.byteOffset, body.byteLength)
            this.at = 0
        }

        need(n) {
            if (this.at + n > this.bytes.length) throw new Error(`malformed message: ${n} bytes wanted, ${this.bytes.length - this.at} left`)
        }

        u8() {
            this.need(1)
            return this.bytes[this.at++]
        }

        u32() {
            this.need(4)
            const v = this.view.getUint32(this.at, true)
            this.at += 4
            return v
        }

        f32() {
            this.need(4)
            const v = this.view.getFloat32(this.at, true)
            this.at += 4
            return v
        }

        string() {
            const n = this.u32()
            this.need(n)
            const s = utf8.decode(this.bytes.subarray(this.at, this.at + n))
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

        u32(v) {
            this.bytes.push(v & 0xff, (v >>> 8) & 0xff, (v >>> 16) & 0xff, (v >>> 24) & 0xff)
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
            default:
                return null
        }
    }

    return {KIND, encode, decode}
})()
