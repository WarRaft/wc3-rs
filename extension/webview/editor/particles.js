'use strict'

/**
 * Particles, as the editor's process described each emitter (see
 * `editor/src/particles.rs`): no particle is kept here. A particle is its
 * number alone: the vertex shader works out when it was made (the
 * emitter makes its particles one after another, the oldest made again
 * when it dies), where it went (a random place in the emitter's width
 * and length, a direction within its latitude, its speed varied, all
 * from the number and the round it's in), and where it is now (gravity
 * down), its colour, alpha and size between its segments', its texture's
 * cell by its age; a square facing the camera. One draw an emitter of a
 * model, a particle of each thing the model is placed as an instance.
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
    // The emitter now: its place in the model, shown, how many a second
    // (each thing at its own place in the loop, as its model's bones).
    float phase = fract(sin(dot(a_place.xy, vec2(12.9898, 78.233))) * 43758.5453) * float(u_frameCount);
    int frame = int(mod(floor(u_time * u_fps + phase), float(u_frameCount)));
    vec4 r0 = texelFetch(u_frames, ivec2(0, frame), 0);
    vec4 r1 = texelFetch(u_frames, ivec2(1, frame), 0);
    vec4 r2 = texelFetch(u_frames, ivec2(2, frame), 0);
    vec4 extra = texelFetch(u_frames, ivec2(3, frame), 0);
    float rate = extra.y;
    if (extra.x < 0.5 || rate <= 0.0) return;
    float life = u_p[4];
    // Made one after another: this one's round and age.
    float every = 1.0 / rate;
    float period = float(alive) * every;
    float now = u_time + fract(phase * 0.618) * period;
    float firstAt = float(slot) * every;
    float cycle = floor((now - firstAt) / period);
    float age = now - (cycle * period + firstAt);
    if (age < 0.0 || age > life) return;
    uint s = uint(slot) * 747796405u + uint(int(cycle)) * 2891336453u + uint(thing) * 1181783497u + 1u;
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
    int c = cell(u_head[h], u_head[h + 1], u_head[h + 2], t);
    float rows = u_p[8];
    float cols = u_p[9];
    vec2 corner = C[gl_VertexID];
    v_uv = (vec2(float(c % int(cols)), float(c / int(cols))) + (corner * vec2(1.0, -1.0) + 1.0) * 0.5) / vec2(cols, rows);
    // Into the world as the thing stands: scaled, turned, moved.
    float cs = cos(a_place.w);
    float sn = sin(a_place.w);
    vec3 q = p * a_scale;
    vec3 world = vec3(q.x * cs - q.y * sn, q.x * sn + q.y * cs, q.z) + a_place.xyz;
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

    /** `particles::FRAME`. */
    const FRAME = 16

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
            const p = gl.createProgram()
            gl.attachShader(p, shader(gl.VERTEX_SHADER, VS))
            gl.attachShader(p, shader(gl.FRAGMENT_SHADER, FS))
            gl.linkProgram(p)
            if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p))
            this.program = p
            this.u = {}
            for (const name of ['u_mvp', 'u_right', 'u_up', 'u_time', 'u_p', 'u_seg', 'u_head', 'u_frames', 'u_frameCount', 'u_fps', 'u_texture', 'u_team', 'u_teamColor', 'u_alphaKey']) {
                this.u[name] = gl.getUniformLocation(p, name)
            }
            /** Each model's emitters: their vertex array (the model's instances, a step every particle), frames texture, what they are. */
            this.models = []
            this.count = 0
        }

        /** The emitters of models `msg.models`, added at `msg.firstModel`; `instanceBuffers` the models' instances. */
        set(msg, instanceBuffers) {
            const gl = this.gl
            if (msg.firstModel === 0) this.clear()
            this.models.length = msg.firstModel
            msg.models.forEach((m, k) => {
                const ib = instanceBuffers[msg.firstModel + k]
                this.models.push(ib ? m.emitters.map(e => this.emitter(e, ib)) : [])
            })
            this.count = this.models.reduce((n, list) => n + list.length, 0)
        }

        emitter(e, ib) {
            const gl = this.gl
            const frames = e.frames.length / FRAME
            const t = gl.createTexture()
            gl.bindTexture(gl.TEXTURE_2D, t)
            gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA32F, 4, frames, 0, gl.RGBA, gl.FLOAT, e.frames)
            for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.NEAREST)
            const alive = Math.max(1, e.params[13])
            const vao = gl.createVertexArray()
            gl.bindVertexArray(vao)
            gl.bindBuffer(gl.ARRAY_BUFFER, ib)
            // `placed::INSTANCE` floats a thing; a step every `alive` particles.
            for (const [location, size, offset] of [[3, 4, 0], [4, 3, 4]]) {
                gl.enableVertexAttribArray(location)
                gl.vertexAttribPointer(location, size, gl.FLOAT, false, 16 * 4, offset * 4)
                gl.vertexAttribDivisor(location, alive)
            }
            gl.bindVertexArray(null)
            return {e, frames, texture: t, vao, alive}
        }

        clear() {
            const gl = this.gl
            for (const list of this.models) for (const x of list) {
                gl.deleteTexture(x.texture)
                gl.deleteVertexArray(x.vao)
            }
            this.models = []
            this.count = 0
        }

        /** Draws them: `things(i)` how many of model `i` stand, `texture(i)` a texture, `camera` its right and up. */
        draw(mvp, time, camera, things, texture) {
            if (!this.count) return
            const gl = this.gl
            const u = this.u
            gl.useProgram(this.program)
            gl.uniformMatrix4fv(u.u_mvp, false, mvp)
            gl.uniform3fv(u.u_right, camera.right)
            gl.uniform3fv(u.u_up, camera.up)
            gl.uniform1f(u.u_time, time)
            gl.uniform1i(u.u_frames, 1)
            gl.uniform1i(u.u_texture, 0)
            gl.enable(gl.BLEND)
            gl.enable(gl.DEPTH_TEST)
            gl.depthMask(false)
            this.models.forEach((list, mi) => {
                const n = things(mi)
                if (!n) return
                for (const x of list) {
                    const p = x.e.params
                    gl.uniform1fv(u.u_p, p)
                    gl.uniform1fv(u.u_seg, x.e.segments)
                    gl.uniform1fv(u.u_head, x.e.head)
                    gl.uniform1i(u.u_frameCount, x.frames)
                    gl.uniform1f(u.u_fps, x.e.fps)
                    gl.uniform1i(u.u_team, x.e.texture === -1 ? 1 : 0)
                    gl.uniform3fv(u.u_teamColor, [1, 1, 1])
                    gl.uniform1i(u.u_alphaKey, p[10] === 4 ? 1 : 0)
                    // The game's filter modes: blend, additive, modulate, modulate 2x, alpha key.
                    switch (p[10]) {
                        case 0: gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); break
                        case 2: gl.blendFunc(gl.ZERO, gl.SRC_COLOR); break
                        case 3: gl.blendFunc(gl.DST_COLOR, gl.SRC_COLOR); break
                        default: gl.blendFunc(gl.SRC_ALPHA, gl.ONE)
                    }
                    gl.activeTexture(gl.TEXTURE1)
                    gl.bindTexture(gl.TEXTURE_2D, x.texture)
                    gl.activeTexture(gl.TEXTURE0)
                    gl.bindTexture(gl.TEXTURE_2D, texture(x.e.texture))
                    gl.bindVertexArray(x.vao)
                    gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, n * x.alive)
                }
            })
            gl.bindVertexArray(null)
            gl.depthMask(true)
            gl.disable(gl.BLEND)
        }
    }

    return {Particles}
})()
