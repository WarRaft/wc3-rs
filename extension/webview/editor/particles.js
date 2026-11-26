'use strict'

/**
 * Particles, as the editor's process described each emitter (see
 * `editor/src/particles.rs`): no particle is kept here. A particle is its
 * number alone: the vertex shader works out when it was made (the
 * emitter makes its particles one after another at its most a second,
 * the oldest made again when it dies; one made while the emitter was
 * hidden, or made fewer of then, left out), where the emitter was then
 * (or is now, for one in the model's space, 0x80000), where it went (a random place in the emitter's width
 * and length, a direction within its latitude, its speed varied, all
 * from the number and the round it's in), and where it is now (gravity
 * down), its colour, alpha and size between its segments', its texture's
 * cell by its age; its head a square facing the camera, its tail a quad
 * from it back along its velocity. One draw an emitter of a model (two
 * with a head and a tail), a particle of each thing the model is placed
 * as an instance.
 *
 * Ribbons (`RIBB`) likewise: a segment is the time it was made, the
 * emitter's place then (the frames the process baked), its height above
 * and below it, gravity since; a quad joins each to the one before, the
 * newest at the emitter now.
 */
const EditorParticles = (() => {
    const VS = `#version 300 es
precision highp float;
precision highp int;
layout(location = 3) in vec4 a_place;
layout(location = 4) in vec3 a_scale;
uniform mat4 u_mvp;
uniform vec3 u_right;
uniform vec3 u_up;
uniform float u_time;
uniform float u_p[16];
uniform float u_seg[15];
uniform float u_head[6];
uniform float u_tail[6];
uniform int u_pass;
uniform highp sampler2D u_frames;
uniform int u_frameCount;
uniform float u_fps;
out vec2 v_uv;
out vec4 v_color;
const vec2 C[6] = vec2[6](vec2(-1, 1), vec2(-1, -1), vec2(1, 1), vec2(1, 1), vec2(-1, -1), vec2(1, -1));
uint hash(uint x) {
    x ^= x >> 16; x *= 0x7feb352du; x ^= x >> 15; x *= 0x846ca68bu; x ^= x >> 16;
    return x;
}
float rnd(inout uint s) { s = hash(s); return float(s & 0xffffffu) / 8388607.5 - 1.0; }
int cell(float start, float end, float repeat, float t) {
    float r = repeat == 0.0 ? 1.0 : repeat;
    float first = end >= start ? start : start + 1.0;
    float delta = end >= start ? end - start + 1.0 : end - start - 1.0;
    float u = r == 1.0 ? t : fract(t * r);
    return max(0, int(first + delta * u));
}
void main() {
    int alive = int(u_p[13]);
    int slot = gl_InstanceID % alive;
    int thing = gl_InstanceID / alive;
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    // Each thing at its own place in the loop, as its model's bones.
    float phase = fract(sin(dot(a_place.xy, vec2(12.9898, 78.233))) * 43758.5453) * float(u_frameCount);
    float life = u_p[4];
    // Made one after another at the emitter's most a second (alive over
    // life); fewer kept when it makes fewer then (its rate at the time).
    float every = life / float(alive);
    float period = float(alive) * every;
    float now = u_time + fract(phase * 0.618) * period;
    float firstAt = float(slot) * every;
    float cycle = floor((now - firstAt) / period);
    float age = now - (cycle * period + firstAt);
    if (age < 0.0 || age > life) return;
    uint s = uint(slot) * 747796405u + uint(int(cycle)) * 2891336453u + uint(thing) * 1181783497u + 1u;
    // The emitter when it was made: shown, making it.
    int made = int(mod(floor((u_time - age) * u_fps + phase), float(u_frameCount)));
    vec4 then = texelFetch(u_frames, ivec2(3, made), 0);
    float keep = rnd(s) * 0.5 + 0.5;
    if (then.x < 0.5 || then.y <= 0.0 || keep > then.y * every) return;
    // Where the emitter is: when it was made (in the world), or now (in
    // the model's space, 0x80000: it goes with the emitter).
    bool modelSpace = (int(u_p[11]) & 0x80000) != 0;
    int frame = modelSpace ? int(mod(floor(u_time * u_fps + phase), float(u_frameCount))) : made;
    vec4 r0 = texelFetch(u_frames, ivec2(0, frame), 0);
    vec4 r1 = texelFetch(u_frames, ivec2(1, frame), 0);
    vec4 r2 = texelFetch(u_frames, ivec2(2, frame), 0);
    vec3 local = vec3(u_p[6] * 0.5 * rnd(s), u_p[5] * 0.5 * rnd(s), 0.0);
    float rotY = u_p[2] * rnd(s);
    bool line = (int(u_p[11]) & 0x20000) != 0;
    float rotZ = line ? 0.0 : 3.14159265 * rnd(s);
    float speed = u_p[0] * (1.0 + u_p[1] * rnd(s));
    vec3 vel = speed * vec3(sin(rotY) * cos(rotZ), sin(rotY) * sin(rotZ), cos(rotY));
    vec4 at = vec4(local, 1.0);
    vec3 start = vec3(dot(r0, at), dot(r1, at), dot(r2, at));
    vel = vec3(dot(r0.xyz, vel), dot(r1.xyz, vel), dot(r2.xyz, vel));
    vec3 p = start + vel * age;
    p.z -= 0.5 * u_p[3] * age * age;
    // Its look between its segments.
    float mid = u_p[7] * life;
    bool early = age <= mid;
    float t = early ? (mid > 0.0 ? age / mid : 0.0) : (life > mid ? (age - mid) / (life - mid) : 0.0);
    t = t * 0.99 + 0.005;
    int a = early ? 0 : 1;
    vec3 colorA = vec3(u_seg[a * 3], u_seg[a * 3 + 1], u_seg[a * 3 + 2]);
    vec3 colorB = vec3(u_seg[a * 3 + 3], u_seg[a * 3 + 4], u_seg[a * 3 + 5]);
    float alpha = mix(u_seg[9 + a], u_seg[10 + a], t);
    float size = mix(u_seg[12 + a], u_seg[13 + a], t);
    v_color = vec4(mix(colorA, colorB, t), alpha);
    int h = early ? 0 : 3;
    int c = u_pass == 1 ? cell(u_tail[h], u_tail[h + 1], u_tail[h + 2], t) : cell(u_head[h], u_head[h + 1], u_head[h + 2], t);
    float rows = u_p[8];
    float cols = u_p[9];
    vec2 corner = C[gl_VertexID];
    v_uv = (vec2(float(c % int(cols)), float(c / int(cols))) + (corner * vec2(1.0, -1.0) + 1.0) * 0.5) / vec2(cols, rows);
    // Into the world as the thing stands: scaled, turned, moved.
    float cs = cos(a_place.w);
    float sn = sin(a_place.w);
    vec3 q = p * a_scale;
    vec3 world = vec3(q.x * cs - q.y * sn, q.x * sn + q.y * cs, q.z) + a_place.xyz;
    float halfSize0 = size * a_scale.x;
    if (u_pass == 1) {
        // The tail: back along its velocity now, tail length seconds of it.
        vec3 v = vel;
        v.z -= u_p[3] * age;
        vec3 q2 = (p - v * u_p[15]) * a_scale;
        vec3 tailWorld = vec3(q2.x * cs - q2.y * sn, q2.x * sn + q2.y * cs, q2.z) + a_place.xyz;
        vec3 axis = world - tailWorld;
        vec3 side = cross(axis, cross(u_right, u_up));
        side = length(side) > 1e-5 ? normalize(side) : u_right;
        vec3 end = corner.y > 0.0 ? world : tailWorld;
        gl_Position = u_mvp * vec4(end + side * corner.x * halfSize0, 1.0);
        return;
    }
    bool flatQuad = (int(u_p[11]) & 0x100000) != 0;
    vec3 right = flatQuad ? vec3(1.0, 0.0, 0.0) : u_right;
    vec3 up = flatQuad ? vec3(0.0, 1.0, 0.0) : u_up;
    float halfSize = size * a_scale.x;
    gl_Position = u_mvp * vec4(world + (corner.x * right + corner.y * up) * halfSize, 1.0);
}`

    const FS = `#version 300 es
precision highp float;
uniform sampler2D u_texture;
uniform bool u_team;
uniform vec3 u_teamColor;
uniform bool u_alphaKey;
in vec2 v_uv;
in vec4 v_color;
out vec4 color;
void main() {
    vec4 t = u_team ? vec4(u_teamColor, 1.0) : texture(u_texture, v_uv);
    color = t * v_color;
    if (u_alphaKey && color.a < 0.75) discard;
}`

    const RIBBON_VS = `#version 300 es
precision highp float;
precision highp int;
layout(location = 3) in vec4 a_place;
layout(location = 4) in vec3 a_scale;
uniform mat4 u_mvp;
uniform float u_time;
uniform float u_r[8];
uniform vec3 u_color;
uniform highp sampler2D u_frames;
uniform int u_frameCount;
uniform float u_fps;
out vec2 v_uv;
out vec4 v_color;
const vec2 C[6] = vec2[6](vec2(-1, 1), vec2(-1, -1), vec2(1, 1), vec2(1, 1), vec2(-1, -1), vec2(1, -1));
void main() {
    int quads = int(u_r[7]) - 1;
    int k = gl_InstanceID % quads;
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    vec2 corner = C[gl_VertexID];
    // The quad's points: k (the newer) and k + 1; this vertex's.
    int j = k + (corner.x > 0.0 ? 1 : 0);
    float rate = u_r[1];
    float life = u_r[0];
    float phase = fract(sin(dot(a_place.xy, vec2(12.9898, 78.233))) * 43758.5453) * float(u_frameCount);
    float now = u_time;
    // Point 0 is the emitter now; point j made at the j-th tick back.
    float made = j == 0 ? now : (floor(now * rate) - float(j - 1)) / rate;
    float age = now - made;
    float older = (floor(now * rate) - float(k)) / rate;
    if (now - older > life) return;
    int frame = int(mod(floor(made * u_fps + phase), float(u_frameCount)));
    vec4 r0 = texelFetch(u_frames, ivec2(0, frame), 0);
    vec4 r1 = texelFetch(u_frames, ivec2(1, frame), 0);
    vec4 r2 = texelFetch(u_frames, ivec2(2, frame), 0);
    vec4 extra = texelFetch(u_frames, ivec2(3, frame), 0);
    if (extra.x < 0.5) return;
    vec4 at = vec4(0.0, corner.y > 0.0 ? extra.y : -extra.z, 0.0, 1.0);
    vec3 p = vec3(dot(r0, at), dot(r1, at), dot(r2, at));
    p.z -= 0.5 * u_r[5] * age * age;
    float cols = u_r[3];
    float rows = u_r[2];
    float slot = u_r[4];
    float col = mod(slot, cols);
    float row = floor(slot / cols);
    // Along it by age, the newest at the cell's left (as Retera's Model Studio).
    v_uv = vec2((col + clamp(age / life, 0.0, 1.0)) / cols, (row + (corner.y > 0.0 ? 0.0 : 1.0)) / rows);
    v_color = vec4(u_color, extra.w);
    float cs = cos(a_place.w);
    float sn = sin(a_place.w);
    vec3 q = p * a_scale;
    vec3 world = vec3(q.x * cs - q.y * sn, q.x * sn + q.y * cs, q.z) + a_place.xyz;
    gl_Position = u_mvp * vec4(world, 1.0);
}`

    /** `particles::FRAME`, `particles::RIBBON_FRAME`. */
    const FRAME = 16
    const RIBBON_FRAME = 16

    class Particles {
        /** @param {WebGL2RenderingContext} gl */
        constructor(gl) {
            this.gl = gl
            const shader = (type, src) => {
                const s = gl.createShader(type)
                gl.shaderSource(s, src)
                gl.compileShader(s)
                if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s))
                return s
            }
            const program = (vs, names) => {
                const p = gl.createProgram()
                gl.attachShader(p, shader(gl.VERTEX_SHADER, vs))
                gl.attachShader(p, shader(gl.FRAGMENT_SHADER, FS))
                gl.linkProgram(p)
                if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p))
                const u = {}
                for (const name of names) u[name] = gl.getUniformLocation(p, name)
                return [p, u]
            }
            const common = ['u_mvp', 'u_time', 'u_frames', 'u_frameCount', 'u_fps', 'u_texture', 'u_team', 'u_teamColor', 'u_alphaKey']
            ;[this.program, this.u] = program(VS, [...common, 'u_right', 'u_up', 'u_p', 'u_seg', 'u_head', 'u_tail', 'u_pass'])
            ;[this.ribbonProgram, this.ru] = program(RIBBON_VS, [...common, 'u_r', 'u_color'])
            /** Each model's ribbons. */
            this.ribbons = []
            this.ribbonCount = 0
            /** Each model's emitters: their vertex array (the model's instances, a step every particle), frames texture, what they are. */
            this.models = []
            this.count = 0
        }

        /** The emitters of models `msg.models`, added at `msg.firstModel`; `instanceBuffers` the models' instances. */
        set(msg, instanceBuffers) {
            const gl = this.gl
            if (msg.firstModel === 0) this.clear()
            this.models.length = msg.firstModel
            this.ribbons.length = msg.firstModel
            msg.models.forEach((m, k) => {
                const ib = instanceBuffers[msg.firstModel + k]
                this.models.push(ib ? m.emitters.map(e => this.emitter(e, ib)) : [])
                this.ribbons.push(ib ? m.ribbons.map(r => this.ribbon(r, ib)) : [])
            })
            this.count = this.models.reduce((n, list) => n + list.length, 0)
            this.ribbonCount = this.ribbons.reduce((n, list) => n + list.length, 0)
        }

        /** A texture of `frames` (`size` floats each, four to a texel). */
        framesTexture(frames, size) {
            const gl = this.gl
            const t = gl.createTexture()
            gl.bindTexture(gl.TEXTURE_2D, t)
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, size / 4, frames.length / size, 0, gl.RGBA, gl.FLOAT, frames)
            for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.NEAREST)
            return t
        }

        /** The model's instances `ib` as a vertex array, a step every `n`. */
        instances(ib, n) {
            const gl = this.gl
            const vao = gl.createVertexArray()
            gl.bindVertexArray(vao)
            gl.bindBuffer(gl.ARRAY_BUFFER, ib)
            for (const [location, size, offset] of [[3, 4, 0], [4, 3, 4]]) {
                gl.enableVertexAttribArray(location)
                gl.vertexAttribPointer(location, size, gl.FLOAT, false, 16 * 4, offset * 4)
                gl.vertexAttribDivisor(location, n)
            }
            gl.bindVertexArray(null)
            return vao
        }

        ribbon(r, ib) {
            const quads = Math.max(1, r.params[7] - 1)
            return {r, frames: r.frames.length / RIBBON_FRAME, texture: this.framesTexture(r.frames, RIBBON_FRAME), vao: this.instances(ib, quads), quads}
        }

        emitter(e, ib) {
            // `placed::INSTANCE` floats a thing; a step every `alive` particles.
            const alive = Math.max(1, e.params[13])
            return {e, frames: e.frames.length / FRAME, texture: this.framesTexture(e.frames, FRAME), vao: this.instances(ib, alive), alive}
        }

        clear() {
            const gl = this.gl
            for (const list of [...this.models, ...this.ribbons]) for (const x of list) {
                gl.deleteTexture(x.texture)
                gl.deleteVertexArray(x.vao)
            }
            this.models = []
            this.ribbons = []
            this.count = 0
            this.ribbonCount = 0
        }

        /** The game's filter modes: blend, additive, modulate, modulate 2x, alpha key. */
        blend(mode) {
            const gl = this.gl
            switch (mode) {
                case 0: gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); break
                case 2: gl.blendFunc(gl.ZERO, gl.SRC_COLOR); break
                case 3: gl.blendFunc(gl.DST_COLOR, gl.SRC_COLOR); break
                case 4: gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); break
                default: gl.blendFunc(gl.SRC_ALPHA, gl.ONE)
            }
        }

        /** The uniforms both programs have, for one emitter or ribbon. */
        common(u, x, mvp, time, textureOf, teamColor, filter) {
            const gl = this.gl
            gl.uniformMatrix4fv(u.u_mvp, false, mvp)
            gl.uniform1f(u.u_time, time)
            gl.uniform1i(u.u_frames, 1)
            gl.uniform1i(u.u_texture, 0)
            gl.uniform1i(u.u_frameCount, x.frames)
            gl.uniform1i(u.u_team, teamColor ? 1 : 0)
            gl.uniform3fv(u.u_teamColor, [1, 1, 1])
            gl.uniform1i(u.u_alphaKey, filter === 4 ? 1 : 0)
            this.blend(filter)
            gl.activeTexture(gl.TEXTURE1)
            gl.bindTexture(gl.TEXTURE_2D, x.texture)
            gl.activeTexture(gl.TEXTURE0)
            gl.bindTexture(gl.TEXTURE_2D, textureOf)
            gl.bindVertexArray(x.vao)
        }

        /** Draws them: `things(i)` how many of model `i` stand, `texture(i)` a texture, `camera` its right and up. */
        draw(mvp, time, camera, things, texture) {
            if (!this.count && !this.ribbonCount) return
            const gl = this.gl
            gl.enable(gl.BLEND)
            gl.enable(gl.DEPTH_TEST)
            gl.depthMask(false)
            gl.useProgram(this.program)
            const u = this.u
            gl.uniform3fv(u.u_right, camera.right)
            gl.uniform3fv(u.u_up, camera.up)
            this.models.forEach((list, mi) => {
                const n = things(mi)
                if (!n) return
                for (const x of list) {
                    const p = x.e.params
                    this.common(u, x, mvp, time, texture(x.e.texture), x.e.texture === -1, p[10])
                    gl.uniform1fv(u.u_p, p)
                    gl.uniform1fv(u.u_seg, x.e.segments)
                    gl.uniform1fv(u.u_head, x.e.head)
                    gl.uniform1fv(u.u_tail, x.e.tail)
                    gl.uniform1f(u.u_fps, x.e.fps)
                    // Its head (0, 2), its tail (1, 2).
                    for (const pass of [0, 1]) {
                        if (pass === 0 ? p[14] === 1 : p[14] === 0) continue
                        gl.uniform1i(u.u_pass, pass)
                        gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, n * x.alive)
                    }
                }
            })
            gl.useProgram(this.ribbonProgram)
            const ru = this.ru
            gl.disable(gl.CULL_FACE)
            this.ribbons.forEach((list, mi) => {
                const n = things(mi)
                if (!n) return
                for (const x of list) {
                    this.common(ru, x, mvp, time, texture(x.r.texture), x.r.texture === -1, x.r.params[6])
                    gl.uniform1fv(ru.u_r, x.r.params)
                    gl.uniform3fv(ru.u_color, x.r.color)
                    gl.uniform1f(ru.u_fps, x.r.fps)
                    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, n * x.quads)
                }
            })
            gl.bindVertexArray(null)
            gl.depthMask(true)
            gl.disable(gl.BLEND)
        }
    }

    return {Particles}
})()
