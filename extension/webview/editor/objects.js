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
 * each one at its own place in the loop; a bone under a billboarded node
 * turned here to face the camera (or about one of its axes) about where
 * the node is.
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
// Reforged's tangents (the fourth the bitangent's sign); (0, 0, 0, 1)
// without them.
layout(location = 9) in vec4 a_tangent;
// 1: one of the things picked together (the instance's last float, the page's).
layout(location = 10) in float a_picked;
uniform mat4 u_mvp;
uniform int u_selected;
// The model's animation: its bones' matrices, a row of four texels a
// bone (three of the matrix, then its billboard and where it's
// anchored), a row of the texture a frame (anim::Baked).
uniform bool u_animated;
uniform bool u_skinned;
// This geoset's colour and alpha in the row (anim::Baked), or -1.
uniform int u_geoset;
uniform bool u_still;
uniform highp sampler2D u_matrices;
uniform float u_time;
uniform float u_fps;
uniform int u_frames;
// The camera's right, up and forward (the world's), for billboards.
uniform vec3 u_camRight;
uniform vec3 u_camUp;
uniform vec3 u_camForward;
vec3 unturn(vec3 v) {
    float c = cos(-a_place.w);
    float s = sin(-a_place.w);
    return vec3(v.x * c - v.y * s, v.x * s + v.y * c, v.z);
}
// A billboard's turn in the model (the thing's own turn taken back):
// its x toward the camera, y to the camera's right, z up; or about its
// x, y or z alone (2, 3, 4).
mat3 facing(int kind) {
    vec3 toward = -unturn(u_camForward);
    vec3 right = unturn(u_camRight);
    vec3 up = unturn(u_camUp);
    vec3 x = toward, y = right, z = up;
    if (kind == 2) {
        x = vec3(1.0, 0.0, 0.0);
        y = normalize(right - x * dot(right, x));
        z = cross(x, y);
    } else if (kind == 3) {
        y = vec3(0.0, 1.0, 0.0);
        x = normalize(toward - y * dot(toward, y) + vec3(1e-5, 0.0, 0.0));
        z = cross(x, y);
    } else if (kind == 4) {
        z = vec3(0.0, 0.0, 1.0);
        x = normalize(vec3(toward.xy, 0.0) + vec3(1e-5, 0.0, 0.0));
        y = cross(z, x);
    }
    return mat3(x, y, z);
}
// Frame x (frames since the start, any sign) of a loop of n, in whole
// numbers: a float mod can round to n itself, past the texture's last
// row (its texels 0: the model gone for a frame).
int loopFrame(float x, int n) {
    int f = int(floor(x));
    if (f < 0) f += n * ((-f) / n + 1);
    return min(f % n, n - 1);
}
vec3 skinned(vec4 v, int frame) {
    vec3 sum = vec3(0.0);
    for (int k = 0; k < 4; k++) {
        float w = a_weights[k];
        if (w <= 0.0) continue;
        int b = int(a_bones[k]) * 4;
        vec4 r0 = texelFetch(u_matrices, ivec2(b, frame), 0);
        vec4 r1 = texelFetch(u_matrices, ivec2(b + 1, frame), 0);
        vec4 r2 = texelFetch(u_matrices, ivec2(b + 2, frame), 0);
        vec4 anchor = texelFetch(u_matrices, ivec2(b + 3, frame), 0);
        vec3 x = vec3(dot(r0, v), dot(r1, v), dot(r2, v));
        int kind = int(anchor.x + 0.5);
        if (kind > 0) x = facing(kind) * x + anchor.yzw * v.w;
        sum += w * x;
    }
    return sum;
}
out vec2 v_uv;
out vec3 v_normal;
out vec4 v_tangent;
out vec3 v_tint;
out vec3 v_team;
out float v_selected;
out float v_dist;
out vec4 v_geoset;
out vec3 v_view;
uniform vec3 u_eye;
void main() {
    vec3 pos = a_pos;
    vec3 normal = a_normal;
    vec3 tangent = a_tangent.xyz;
    v_geoset = vec4(1.0);
    if (u_animated) {
        // Each one at its own place in the loop, by where it stands.
        float phase = fract(sin(dot(a_place.xy, vec2(12.9898, 78.233))) * 43758.5453) * float(u_frames);
        int frame = u_still ? 0 : loopFrame(u_time * u_fps + phase, u_frames);
        if (u_skinned) {
            pos = skinned(vec4(a_pos, 1.0), frame);
            normal = skinned(vec4(a_normal, 0.0), frame);
            tangent = skinned(vec4(a_tangent.xyz, 0.0), frame);
        }
        if (u_geoset >= 0) v_geoset = texelFetch(u_matrices, ivec2(u_geoset, frame), 0);
    }
    vec3 p = pos * a_scale;
    float c = cos(a_place.w);
    float s = sin(a_place.w);
    p = vec3(p.x * c - p.y * s, p.x * s + p.y * c, p.z) + a_place.xyz;
    v_normal = vec3(normal.x * c - normal.y * s, normal.x * s + normal.y * c, normal.z);
    v_tangent = vec4(tangent.x * c - tangent.y * s, tangent.x * s + tangent.y * c, tangent.z, a_tangent.w);
    v_uv = a_uv;
    v_tint = a_tint;
    v_team = a_team;
    v_selected = gl_InstanceID == u_selected || a_picked > 0.5 ? 1.0 : 0.0;
    v_dist = distance(p, u_eye) * 128.0;
    v_view = u_eye - p;
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
// Reforged's HD layer: its normal map (x, y), ORM (occlusion, roughness,
// metalness; its alpha where team colour goes), emissive, reflections
// (units 2, 3, 4, 5).
uniform bool u_hd;
uniform sampler2D u_normalMap;
uniform sampler2D u_orm;
uniform sampler2D u_emissive;
uniform sampler2D u_reflections;
uniform vec2 u_viewport;
in vec3 v_view;
${EditorLight.GLSL}
in float v_dist;
in vec2 v_uv;
in vec3 v_normal;
in vec4 v_tangent;
in vec3 v_tint;
in vec3 v_team;
in float v_selected;
in vec4 v_geoset;
out vec4 color;
void main() {
    // Hidden as its animation says at this frame.
    if (v_geoset.a <= 0.001) discard;
    color = u_source == 1 ? vec4(v_team, 1.0) : texture(u_texture, v_uv);
    color.rgb *= v_tint * v_geoset.rgb;
    color.a *= u_alpha * v_geoset.a;
    if (u_cutout && color.a < 0.75) discard;
    vec3 n = v_normal;
    vec3 glow = vec3(0.0);
    vec4 orm = vec4(1.0, 1.0, 0.0, 0.0);
    if (u_hd) {
        // As the game's HD shader: team colour where the ORM's alpha says,
        // the normal map's x and y (z worked out) about the vertex's
        // tangent, the emissive added after the light.
        orm = texture(u_orm, v_uv);
        color.rgb = mix(color.rgb, color.rgb * v_team, orm.a);
        if (dot(v_tangent.xyz, v_tangent.xyz) > 1e-4) {
            vec2 xy = texture(u_normalMap, v_uv).xy * 2.0 - 1.0;
            vec3 tn = vec3(xy, sqrt(max(0.0, 1.0 - dot(xy, xy))));
            vec3 N = normalize(v_normal);
            vec3 T = normalize(v_tangent.xyz - N * dot(N, v_tangent.xyz));
            vec3 B = cross(N, T) * (v_tangent.w < 0.0 ? -1.0 : 1.0);
            n = T * tn.x + B * tn.y + N * tn.z;
        }
        glow = texture(u_emissive, v_uv).rgb;
    }
    if (!u_unshaded) {
        vec3 base = color.rgb;
        color.rgb = lit(color.rgb, n, clamp((dot(-u_light, normalize(n)) + 1.0) * 0.5, 0.0, 1.0) * 1.2);
        if (u_hd) {
            // The highlight as Retera's Model Studio draws it: about the
            // half vector, as strong as the surface is smooth (G) and
            // metal (B), its colour the reflections' (an environment map
            // at the pixel's place on the screen) or, the rougher, the
            // surface's.
            vec3 N = normalize(n);
            vec3 H = normalize(-u_light + normalize(v_view));
            float spec = pow(max(dot(N, H) * 0.5 + 0.5, 0.0), 32.0);
            vec3 sky = clamp(0.2 + 2.0 * texture(u_reflections, gl_FragCoord.xy / u_viewport * vec2(1.0, -1.0)).rgb, 0.0, 1.0);
            color.rgb += (max(0.5 - orm.g, 0.0) + orm.b) * spec * mix(sky, base, orm.g);
        }
    }
    color.rgb += glow;
    color.rgb = fogged(color.rgb, v_dist);
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
        const light = Object.fromEntries(['u_daylight', 'u_sun', 'u_diffuse', 'u_ambient', 'u_fog', 'u_fogStart', 'u_fogEnd', 'u_fogColor', 'u_eye'].map(n => [n, u(n)]))
        return {...light, program: p, u_animated: u('u_animated'), u_skinned: u('u_skinned'), u_geoset: u('u_geoset'), u_still: u('u_still'), u_camRight: u('u_camRight'), u_camUp: u('u_camUp'), u_camForward: u('u_camForward'), u_matrices: u('u_matrices'), u_time: u('u_time'), u_fps: u('u_fps'), u_frames: u('u_frames'), u_mvp: u('u_mvp'), u_selected: u('u_selected'), u_texture: u('u_texture'), u_source: u('u_source'), u_alpha: u('u_alpha'), u_cutout: u('u_cutout'), u_unshaded: u('u_unshaded'), u_light: u('u_light'), u_hd: u('u_hd'), u_normalMap: u('u_normalMap'), u_orm: u('u_orm'), u_emissive: u('u_emissive'), u_reflections: u('u_reflections'), u_viewport: u('u_viewport')}
    }

    class Objects {
        /** @param {WebGL2RenderingContext} gl */
        constructor(gl) {
            this.gl = gl
            this.program = program(gl)
            this.textures = []
            this.models = []
            this.particles = new EditorParticles.Particles(gl)
            this.white = this.texture({width: 1, height: 1, rgba: new Uint8Array([255, 255, 255, 255])})
            // An HD layer's slots it hasn't: a flat normal, an ORM with no
            // team colour, no glow.
            this.flat = this.texture({width: 1, height: 1, rgba: new Uint8Array([128, 128, 255, 255])})
            this.plainOrm = this.texture({width: 1, height: 1, rgba: new Uint8Array([255, 255, 0, 0])})
            this.black = this.texture({width: 1, height: 1, rgba: new Uint8Array([0, 0, 0, 255])})
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
                const model = {path: m.path, geosets: [], instances: 0, instanceBuffer: null}
                this.models.push(model)
                if (m.error) {
                    this.notes.push(m.error)
                    continue
                }
                const ib = gl.createBuffer()
                model.instanceBuffer = ib
                if (m.anim.frames) {
                    // Three texels a bone, a row a frame.
                    const t = gl.createTexture()
                    gl.bindTexture(gl.TEXTURE_2D, t)
                    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, m.anim.bones * 4 + m.anim.geosets, m.anim.frames, 0, gl.RGBA, gl.FLOAT, m.anim.matrices)
                    for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.NEAREST)
                    model.matrices = t
                    model.anim = {fps: m.anim.fps, frames: m.anim.frames, bones: m.anim.bones}
                    // Any bone of its first frame a billboard's (`anim::BONE`: the 13th float).
                    model.billboards = Array.from({length: m.anim.bones}, (_, b) => m.anim.matrices[b * 16 + 12]).some(k => k > 0)
                    // A geoset's alpha or colour not 1 in some frame: animated geosets.
                    const row = m.anim.bones * 16 + m.anim.geosets * 4
                    model.geosetAnimated = Array.from({length: m.anim.frames * m.anim.geosets * 4}, (_, i) => m.anim.matrices[Math.floor(i / (m.anim.geosets * 4)) * row + m.anim.bones * 16 + (i % (m.anim.geosets * 4))]).some(v => v !== 1)
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
                    if (g.tangents.length) attribute(9, g.tangents, 4)
                    else gl.vertexAttrib4f(9, 0, 0, 0, 1)
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
                    for (const [location, size, offset] of [[3, 4, 0], [4, 3, 4], [5, 3, 7], [6, 3, 10], [10, 1, 15]]) {
                        gl.enableVertexAttribArray(location)
                        gl.vertexAttribPointer(location, size, gl.FLOAT, false, stride, offset * 4)
                        gl.vertexAttribDivisor(location, 1)
                    }
                    const eb = gl.createBuffer()
                    buffers.push(eb)
                    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, eb)
                    gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, g.indices, gl.STATIC_DRAW)
                    gl.bindVertexArray(null)
                    model.geosets.push({vao, buffers, count: g.indices.length, layers: g.layers, index: g.index})
                }
            }
            this.particles.set(msg, this.models.map(m => m.instanceBuffer))
        }

        /** Where every model stands: a list, a model each, of its instances. */
        setInstances(list) {
            const gl = this.gl
            this.instanceData = list
            this.markGroup(list)
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

        /** The things picked together (`file:index` keys), shown as the one picked is. */
        setGroup(keys) {
            this.group = keys
            const gl = this.gl
            ;(this.instanceData ?? []).forEach((data, i) => {
                const m = this.models[i]
                if (!this.markGroup([data]) || !m?.instanceBuffer) return
                gl.bindBuffer(gl.ARRAY_BUFFER, m.instanceBuffer)
                gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW)
            })
        }

        /** The group's flag in each instance of `list`; whether one changed. */
        markGroup(list) {
            let changed = false
            list.forEach(data => {
                for (let i = 0; i < data.length / INSTANCE; i++) {
                    const at = i * INSTANCE
                    const key = `${data[at + 13] < 2 ? 0 : 1}:${data[at + 14]}`
                    const flag = this.group?.has(key) ? 1 : 0
                    if (data[at + 15] !== flag) {
                        data[at + 15] = flag
                        changed = true
                    }
                }
            })
            return changed
        }

        /** The things whose place `mvp` puts inside the box of clip space [x0, y0, x1, y1]: (file, index) each. */
        inside(mvp, [x0, y0, x1, y1]) {
            const out = []
            for (const data of this.instanceData ?? []) {
                for (let i = 0; i < data.length / INSTANCE; i++) {
                    const at = i * INSTANCE
                    const [x, y, z] = [data[at], data[at + 1], data[at + 2]]
                    const w = mvp[3] * x + mvp[7] * y + mvp[11] * z + mvp[15]
                    if (w <= 0) continue
                    const cx = (mvp[0] * x + mvp[4] * y + mvp[8] * z + mvp[12]) / w
                    const cy = (mvp[1] * x + mvp[5] * y + mvp[9] * z + mvp[13]) / w
                    if (cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1) out.push({file: data[at + 13] < 2 ? 0 : 1, index: data[at + 14]})
                }
            }
            return out
        }

        draw(mvp, time = 0, camera = null, light = null, hours = 12) {
            const gl = this.gl
            const p = this.program
            gl.useProgram(p.program)
            gl.uniformMatrix4fv(p.u_mvp, false, mvp)
            gl.uniform3fv(p.u_light, LIGHT)
            gl.uniform1i(p.u_texture, 0)
            gl.uniform1i(p.u_matrices, 1)
            gl.uniform1f(p.u_time, time)
            gl.uniform3fv(p.u_eye, camera?.eye ?? [0, 0, 0])
            gl.uniform3fv(p.u_camRight, camera?.right ?? [1, 0, 0])
            gl.uniform3fv(p.u_camUp, camera?.up ?? [0, 0, 1])
            gl.uniform3fv(p.u_camForward, camera?.view ?? [0, 1, 0])
            // Not animated: the first frame still (billboards face the camera all the same).
            gl.uniform1i(p.u_still, this.animate ? 0 : 1)
            if (light) light.apply(gl, p, 'units', hours)
            else gl.uniform1i(p.u_daylight, 0)
            gl.activeTexture(gl.TEXTURE0)
            gl.enable(gl.DEPTH_TEST)
            for (const pass of [0, 1]) {
                this.models.forEach((m, mi) => {
                    if (!m.instances) return
                    gl.uniform1i(p.u_selected, this.selected.model === mi ? this.selected.instance : -1)
                    gl.uniform1i(p.u_animated, m.anim ? 1 : 0)
                    gl.uniform1i(p.u_skinned, m.anim?.bones ? 1 : 0)
                    if (m.anim) {
                        gl.uniform1f(p.u_fps, m.anim.fps)
                        gl.uniform1i(p.u_frames, m.anim.frames)
                        gl.activeTexture(gl.TEXTURE1)
                        gl.bindTexture(gl.TEXTURE_2D, m.matrices)
                        gl.activeTexture(gl.TEXTURE0)
                    }
                    for (const g of m.geosets) {
                        gl.bindVertexArray(g.vao)
                        gl.uniform1i(p.u_geoset, m.anim ? m.anim.bones * 4 + g.index : -1)
                        for (const l of g.layers) {
                            const translucent = l.filterMode >= 2
                            if (translucent !== (pass === 1) || l.texture < TEAM_COLOR) continue
                            this.blend(l.filterMode)
                            gl.uniform1i(p.u_source, l.texture === TEAM_COLOR ? 1 : 0)
                            gl.bindTexture(gl.TEXTURE_2D, l.texture >= 0 ? this.textures[l.texture] ?? this.white : this.white)
                            gl.uniform1f(p.u_alpha, l.alpha)
                            gl.uniform1i(p.u_cutout, l.filterMode === 1 ? 1 : 0)
                            gl.uniform1i(p.u_unshaded, l.shading & 1 ? 1 : 0)
                            gl.uniform1i(p.u_hd, l.hd ? 1 : 0)
                            if (l.hd) {
                                const own = (i, none) => (i >= 0 ? this.textures[i] : null) ?? none
                                for (const [unit, name, t] of [[2, 'u_normalMap', own(l.normal, this.flat)], [3, 'u_orm', own(l.orm, this.plainOrm)], [4, 'u_emissive', own(l.emissive, this.black)], [5, 'u_reflections', own(l.reflections, this.black)]]) {
                                    gl.activeTexture(gl.TEXTURE0 + unit)
                                    gl.bindTexture(gl.TEXTURE_2D, t)
                                    gl.uniform1i(p[name], unit)
                                }
                                gl.uniform2f(p.u_viewport, gl.drawingBufferWidth, gl.drawingBufferHeight)
                                gl.activeTexture(gl.TEXTURE0)
                            }
                            gl.drawElementsInstanced(gl.TRIANGLES, g.count, gl.UNSIGNED_SHORT, 0, m.instances)
                        }
                    }
                })
            }
            gl.bindVertexArray(null)
            gl.disable(gl.BLEND)
            gl.depthMask(true)
            if (camera && this.animate) {
                this.particles.draw(mvp, time, camera, i => this.models[i]?.instances ?? 0, i => (i >= 0 ? this.textures[i] : null) ?? this.white)
            }
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
