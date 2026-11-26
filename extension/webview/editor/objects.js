'use strict'

/**
 * Draws what the map places, as the editor's process sent it (see
 * `editor/src/placed.rs`): each model's geosets, instanced at every place
 * it stands (moved, turned, scaled, tinted, in its player's colour), each
 * geoset's material's layers in order, as the game blends them (filter
 * modes: 0 opaque, 1 cut out at alpha 0.75, 2 blended, 3 added, 4 added
 * by alpha, 5 and 6 multiplied). Opaque layers first, the others after
 * with the depth kept. What's picked (`select`) is drawn lighter. A model
 * that moves (its Stand, baked by the editor: `anim::Baked`) has its
 * vertices moved on the GPU by its bones' matrices, a texture of them,
 * each one at its own place in the loop.
 */
const EditorObjects = (() => {
    const VS = `#version 300 es
precision highp float;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
layout(location = 2) in vec2 a_uv;
layout(location = 3) in vec4 a_place;
layout(location = 4) in vec3 a_scale;
layout(location = 5) in vec3 a_tint;
layout(location = 6) in vec3 a_team;
layout(location = 7) in vec4 a_bones;
layout(location = 8) in vec4 a_weights;
uniform mat4 u_mvp;
uniform int u_selected;
// The model's animation: its bones' matrices, a row of three texels a
// bone, a row of the texture a frame (anim::Baked).
uniform bool u_animated;
uniform highp sampler2D u_matrices;
uniform float u_time;
uniform float u_fps;
uniform int u_frames;
vec3 skinned(vec4 v, int frame) {
    vec3 sum = vec3(0.0);
    for (int k = 0; k < 4; k++) {
        float w = a_weights[k];
        if (w <= 0.0) continue;
        int b = int(a_bones[k]) * 3;
        vec4 r0 = texelFetch(u_matrices, ivec2(b, frame), 0);
        vec4 r1 = texelFetch(u_matrices, ivec2(b + 1, frame), 0);
        vec4 r2 = texelFetch(u_matrices, ivec2(b + 2, frame), 0);
        sum += w * vec3(dot(r0, v), dot(r1, v), dot(r2, v));
    }
    return sum;
}
out vec2 v_uv;
out vec3 v_normal;
out vec3 v_tint;
out vec3 v_team;
out float v_selected;
void main() {
    vec3 pos = a_pos;
    vec3 normal = a_normal;
    if (u_animated) {
        // Each one at its own place in the loop, by where it stands.
        float phase = fract(sin(dot(a_place.xy, vec2(12.9898, 78.233))) * 43758.5453) * float(u_frames);
        int frame = int(mod(floor(u_time * u_fps + phase), float(u_frames)));
        pos = skinned(vec4(a_pos, 1.0), frame);
        normal = skinned(vec4(a_normal, 0.0), frame);
    }
    vec3 p = pos * a_scale;
    float c = cos(a_place.w);
    float s = sin(a_place.w);
    p = vec3(p.x * c - p.y * s, p.x * s + p.y * c, p.z) + a_place.xyz;
    v_normal = vec3(normal.x * c - normal.y * s, normal.x * s + normal.y * c, normal.z);
    v_uv = a_uv;
    v_tint = a_tint;
    v_team = a_team;
    v_selected = gl_InstanceID == u_selected ? 1.0 : 0.0;
    gl_Position = u_mvp * vec4(p, 1.0);
}`

    const FS = `#version 300 es
precision highp float;
uniform sampler2D u_texture;
uniform int u_source;
uniform float u_alpha;
uniform bool u_cutout;
uniform bool u_unshaded;
uniform vec3 u_light;
in vec2 v_uv;
in vec3 v_normal;
in vec3 v_tint;
in vec3 v_team;
in float v_selected;
out vec4 color;
void main() {
    color = u_source == 1 ? vec4(v_team, 1.0) : texture(u_texture, v_uv);
    color.rgb *= v_tint;
    color.a *= u_alpha;
    if (u_cutout && color.a < 0.75) discard;
    if (!u_unshaded) {
        color.rgb *= clamp((dot(-u_light, normalize(v_normal)) + 1.0) * 0.5, 0.0, 1.0) * 1.2;
    }
    color.rgb = mix(color.rgb, vec3(0.3, 1.0, 0.3), v_selected * 0.35);
}`

    const LIGHT = [-0.3, -0.3, -0.25].map(v => v / Math.hypot(0.3, 0.3, 0.25))
    /** `placed::texture`. */
    const TEAM_COLOR = -1
    /** Floats per instance (`placed::INSTANCE`). */
    const INSTANCE = 16

    function program(gl) {
        const shader = (type, src) => {
            const s = gl.createShader(type)
            gl.shaderSource(s, src)
            gl.compileShader(s)
            if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s))
            return s
        }
        const p = gl.createProgram()
        gl.attachShader(p, shader(gl.VERTEX_SHADER, VS))
        gl.attachShader(p, shader(gl.FRAGMENT_SHADER, FS))
        gl.linkProgram(p)
        if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p))
        const u = name => gl.getUniformLocation(p, name)
        return {program: p, u_animated: u('u_animated'), u_matrices: u('u_matrices'), u_time: u('u_time'), u_fps: u('u_fps'), u_frames: u('u_frames'), u_mvp: u('u_mvp'), u_selected: u('u_selected'), u_texture: u('u_texture'), u_source: u('u_source'), u_alpha: u('u_alpha'), u_cutout: u('u_cutout'), u_unshaded: u('u_unshaded'), u_light: u('u_light')}
    }

    class Objects {
        /** @param {WebGL2RenderingContext} gl */
        constructor(gl) {
            this.gl = gl
            this.program = program(gl)
            this.textures = []
            this.models = []
            this.white = this.texture({width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 255])})
            /** The model and instance picked; -1 none. */
            this.selected = {model: -1, instance: -1}
            this.notes = []
            this.count = 0
            /** Whether the models move (their Stand); off: as their files have them. */
            this.animate = true
        }

        texture(t) {
            const gl = this.gl
            const tex = gl.createTexture()
            gl.bindTexture(gl.TEXTURE_2D, tex)
            gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4)
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, t.width, t.height, 0, gl.RGBA, gl.UNSIGNED_BYTE, t.rgba)
            gl.generateMipmap(gl.TEXTURE_2D)
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR)
            gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
            return tex
        }

        clear() {
            const gl = this.gl
            this.instanceData = []
            for (const t of this.textures) if (t !== this.white) gl.deleteTexture(t)
            for (const m of this.models) for (const g of m.geosets) {
                gl.deleteVertexArray(g.vao)
                g.buffers.forEach(b => gl.deleteBuffer(b))
            }
            for (const m of this.models) if (m.instanceBuffer) gl.deleteBuffer(m.instanceBuffer)
            for (const m of this.models) if (m.matrices) gl.deleteTexture(m.matrices)
            this.textures = []
            this.models = []
            this.count = 0
        }

        /**
         * Models and textures added at `firstModel`, `firstTexture` (0: all
         * anew); where they stand comes with `instances`.
         */
        set(msg) {
            const gl = this.gl
            if (msg.firstModel === 0 && msg.firstTexture === 0) this.clear()
            this.textures.length = msg.firstTexture
            this.textures.push(...msg.textures.map(t => t.error || !t.width ? this.white : this.texture(t)))
            this.models.length = msg.firstModel
            this.notes = msg.notes
            for (const m of msg.models) {
                const model = {geosets: [], instances: 0, instanceBuffer: null}
                this.models.push(model)
                if (m.error) {
                    this.notes.push(m.error)
                    continue
                }
                const ib = gl.createBuffer()
                model.instanceBuffer = ib
                if (m.anim.bones) {
                    // Three texels a bone, a row a frame.
                    const t = gl.createTexture()
                    gl.bindTexture(gl.TEXTURE_2D, t)
                    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, m.anim.bones * 3, m.anim.frames, 0, gl.RGBA, gl.FLOAT, m.anim.matrices)
                    for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.NEAREST)
                    model.matrices = t
                    model.anim = {fps: m.anim.fps, frames: m.anim.frames}
                }
                for (const g of m.geosets) {
                    const vao = gl.createVertexArray()
                    gl.bindVertexArray(vao)
                    const buffers = []
                    const attribute = (location, data, size) => {
                        const b = gl.createBuffer()
                        buffers.push(b)
                        gl.bindBuffer(gl.ARRAY_BUFFER, b)
                        gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW)
                        gl.enableVertexAttribArray(location)
                        gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0)
                    }
                    attribute(0, g.positions, 3)
                    attribute(1, g.normals, 3)
                    attribute(2, g.uvs, 2)
                    if (g.skin.length) {
                        const b = gl.createBuffer()
                        buffers.push(b)
                        gl.bindBuffer(gl.ARRAY_BUFFER, b)
                        gl.bufferData(gl.ARRAY_BUFFER, g.skin, gl.STATIC_DRAW)
                        gl.enableVertexAttribArray(7)
                        gl.vertexAttribPointer(7, 4, gl.UNSIGNED_BYTE, false, 8, 0)
                        gl.enableVertexAttribArray(8)
                        gl.vertexAttribPointer(8, 4, gl.UNSIGNED_BYTE, true, 8, 4)
                    }
                    gl.bindBuffer(gl.ARRAY_BUFFER, ib)
                    const stride = INSTANCE * 4
                    for (const [location, size, offset] of [[3, 4, 0], [4, 3, 4], [5, 3, 7], [6, 3, 10]]) {
                        gl.enableVertexAttribArray(location)
                        gl.vertexAttribPointer(location, size, gl.FLOAT, false, stride, offset * 4)
                        gl.vertexAttribDivisor(location, 1)
                    }
                    const eb = gl.createBuffer()
                    buffers.push(eb)
                    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, eb)
                    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, g.indices, gl.STATIC_DRAW)
                    gl.bindVertexArray(null)
                    model.geosets.push({vao, buffers, count: g.indices.length, layers: g.layers})
                }
            }
        }

        /** Where every model stands: a list, a model each, of its instances. */
        setInstances(list) {
            const gl = this.gl
            this.instanceData = list
            this.count = 0
            list.forEach((data, i) => {
                const m = this.models[i]
                if (!m?.instanceBuffer) return
                gl.bindBuffer(gl.ARRAY_BUFFER, m.instanceBuffer)
                gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW)
                m.instances = data.length / INSTANCE
                this.count += m.instances
            })
        }

        /** The model and instance of thing `index` of `file` (0 `war3map.doo`, 1 `war3mapUnits.doo`), if it's drawn. */
        find(file, index) {
            for (const [mi, data] of (this.instanceData ?? []).entries()) {
                for (let i = 0; i < data.length / INSTANCE; i++) {
                    const kind = data[i * INSTANCE + 13]
                    if ((kind < 2 ? 0 : 1) === file && data[i * INSTANCE + 14] === index) return {model: mi, instance: i}
                }
            }
            return null
        }

        select(model, instance) {
            this.selected = {model, instance}
        }

        draw(mvp, time = 0) {
            const gl = this.gl
            const p = this.program
            gl.useProgram(p.program)
            gl.uniformMatrix4fv(p.u_mvp, false, mvp)
            gl.uniform3fv(p.u_light, LIGHT)
            gl.uniform1i(p.u_texture, 0)
            gl.uniform1i(p.u_matrices, 1)
            gl.uniform1f(p.u_time, time)
            gl.activeTexture(gl.TEXTURE0)
            gl.enable(gl.DEPTH_TEST)
            for (const pass of [0, 1]) {
                this.models.forEach((m, mi) => {
                    if (!m.instances) return
                    gl.uniform1i(p.u_selected, this.selected.model === mi ? this.selected.instance : -1)
                    gl.uniform1i(p.u_animated, m.anim && this.animate ? 1 : 0)
                    if (m.anim) {
                        gl.uniform1f(p.u_fps, m.anim.fps)
                        gl.uniform1i(p.u_frames, m.anim.frames)
                        gl.activeTexture(gl.TEXTURE1)
                        gl.bindTexture(gl.TEXTURE_2D, m.matrices)
                        gl.activeTexture(gl.TEXTURE0)
                    }
                    for (const g of m.geosets) {
                        gl.bindVertexArray(g.vao)
                        for (const l of g.layers) {
                            const translucent = l.filterMode >= 2
                            if (translucent !== (pass === 1) || l.texture < TEAM_COLOR) continue
                            this.blend(l.filterMode)
                            gl.uniform1i(p.u_source, l.texture === TEAM_COLOR ? 1 : 0)
                            gl.bindTexture(gl.TEXTURE_2D, l.texture >= 0 ? this.textures[l.texture] ?? this.white : this.white)
                            gl.uniform1f(p.u_alpha, l.alpha)
                            gl.uniform1i(p.u_cutout, l.filterMode === 1 ? 1 : 0)
                            gl.uniform1i(p.u_unshaded, l.shading & 1 ? 1 : 0)
                            gl.drawElementsInstanced(gl.TRIANGLES, g.count, gl.UNSIGNED_SHORT, 0, m.instances)
                        }
                    }
                })
            }
            gl.bindVertexArray(null)
            gl.disable(gl.BLEND)
            gl.depthMask(true)
        }

        blend(mode) {
            const gl = this.gl
            if (mode <= 1) {
                gl.disable(gl.BLEND)
                gl.depthMask(true)
                return
            }
            gl.enable(gl.BLEND)
            gl.depthMask(false)
            switch (mode) {
                case 2: gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); break
                case 3: gl.blendFunc(gl.ONE, gl.ONE); break
                case 4: gl.blendFunc(gl.SRC_ALPHA, gl.ONE); break
                case 5: gl.blendFunc(gl.ZERO, gl.SRC_COLOR); break
                default: gl.blendFunc(gl.DST_COLOR, gl.SRC_COLOR)
            }
        }
    }

    return {Objects}
})()
