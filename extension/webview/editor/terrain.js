'use strict'

/**
 * Draws a terrain as the editor's process worked it out (see
 * `editor/src/terrain.rs`): no tile is worked out here. The ground is a
 * quad per tile, its corners' heights fetched from a texture; its colour
 * the tile's up to four ground layers, each over the one before through
 * its blend mask (HiveWE's `terrain.frag`), lit by its ground's normal.
 * The water is a quad per tile with water at a corner, at the water's
 * level, its colour by depth between the tileset's shallow and deep
 * colours, its texture animated.
 */
const EditorTerrain = (() => {
    const GROUND_VS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2D;
precision highp usampler2D;
uniform mat4 u_mvp;
uniform ivec2 u_size;
uniform sampler2D u_heights;
uniform sampler2D u_ground;
uniform usampler2D u_tiles;
uniform usampler2D u_corners;
out vec2 v_uv;
flat out uvec4 v_tex;
out vec3 v_normal;
out vec2 v_pos;
out float v_boundary;
const vec2 P[6] = vec2[6](vec2(1, 1), vec2(0, 1), vec2(0, 0), vec2(0, 0), vec2(1, 0), vec2(1, 1));
void main() {
    int tw = u_size.x - 1;
    ivec2 tile = ivec2(gl_InstanceID % tw, gl_InstanceID / tw);
    vec2 p = P[gl_VertexID];
    ivec2 c = tile + ivec2(p);
    float z = texelFetch(u_heights, c, 0).r;
    float hL = texelFetch(u_ground, ivec2(max(c.x - 1, 0), c.y), 0).r;
    float hR = texelFetch(u_ground, ivec2(min(c.x + 1, u_size.x - 1), c.y), 0).r;
    float hD = texelFetch(u_ground, ivec2(c.x, max(c.y - 1, 0)), 0).r;
    float hU = texelFetch(u_ground, ivec2(c.x, min(c.y + 1, u_size.y - 1)), 0).r;
    v_normal = normalize(vec3(hL - hR, hD - hU, 2.0));
    v_uv = vec2(p.x, 1.0 - p.y);
    v_tex = texelFetch(u_tiles, tile, 0);
    v_pos = vec2(c);
    uint flags = texelFetch(u_corners, c, 0).r;
    v_boundary = (flags & 6u) != 0u ? 1.0 : 0.0;
    gl_Position = u_mvp * vec4(vec2(c), z, 1.0);
}`

    const GROUND_FS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2DArray;
uniform sampler2DArray u_layers;
uniform uint u_first[64];
uniform vec3 u_light;
uniform bool u_grid;
in vec2 v_uv;
flat in uvec4 v_tex;
in vec3 v_normal;
in vec2 v_pos;
in float v_boundary;
out vec4 color;
vec4 layer(uint e) {
    uint t = e & 0xFFFFu;
    if (t == 0xFFFFu) return vec4(0.0);
    return texture(u_layers, vec3(v_uv, float(u_first[t] + (e >> 16))));
}
void main() {
    color = layer(v_tex.a);
    color = mix(layer(v_tex.b), color, color.a);
    color = mix(layer(v_tex.g), color, color.a);
    color = mix(layer(v_tex.r), color, color.a);
    float light = clamp((dot(-u_light, normalize(v_normal)) + 1.0) * 0.5, 0.0, 1.0);
    color.rgb *= light;
    color.rgb *= 1.0 - 0.55 * v_boundary;
    if (u_grid) {
        vec2 g = abs(fract(v_pos - 0.5) - 0.5) / fwidth(v_pos);
        float line = 1.0 - clamp(min(g.x, g.y), 0.0, 1.0);
        color.rgb = mix(color.rgb, vec3(1.0), line * 0.25);
    }
    color.a = 1.0;
}`

    const WATER_VS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2D;
precision highp usampler2D;
uniform mat4 u_mvp;
uniform ivec2 u_size;
uniform sampler2D u_heights;
uniform sampler2D u_water;
uniform usampler2D u_corners;
uniform vec4 u_colors[4];
out vec2 v_uv;
out vec4 v_color;
const vec2 P[6] = vec2[6](vec2(1, 1), vec2(0, 1), vec2(0, 0), vec2(0, 0), vec2(1, 0), vec2(1, 1));
const float MIN_DEPTH = 10.0 / 128.0;
const float DEEP = 64.0 / 128.0;
const float MAX_DEPTH = 72.0 / 128.0;
bool wet(ivec2 c) { return (texelFetch(u_corners, c, 0).r & 1u) != 0u; }
void main() {
    int tw = u_size.x - 1;
    ivec2 tile = ivec2(gl_InstanceID % tw, gl_InstanceID / tw);
    bool any = wet(tile) || wet(tile + ivec2(1, 0)) || wet(tile + ivec2(0, 1)) || wet(tile + ivec2(1, 1));
    vec2 p = P[gl_VertexID];
    ivec2 c = tile + ivec2(p);
    float water = texelFetch(u_water, c, 0).r;
    float ground = texelFetch(u_heights, c, 0).r;
    float value = clamp(water - ground, 0.0, 1.0);
    if (value <= DEEP) {
        value = max(0.0, value - MIN_DEPTH) / (DEEP - MIN_DEPTH);
        v_color = mix(u_colors[0], u_colors[1], value);
    } else {
        value = clamp(value - DEEP, 0.0, MAX_DEPTH - DEEP) / (MAX_DEPTH - DEEP);
        v_color = mix(u_colors[2], u_colors[3], value);
    }
    v_uv = vec2(p.x, 1.0 - p.y);
    gl_Position = any ? u_mvp * vec4(vec2(c), water, 1.0) : vec4(2.0, 0.0, 0.0, 1.0);
}`

    const WATER_FS = `#version 300 es
precision highp float;
precision highp sampler2DArray;
uniform sampler2DArray u_frames;
uniform float u_frame;
uniform bool u_textured;
in vec2 v_uv;
in vec4 v_color;
out vec4 color;
void main() {
    vec4 t = u_textured ? texture(u_frames, vec3(v_uv, u_frame)) : vec4(1.0);
    color = t * v_color;
}`

    function program(gl, vs, fs) {
        const shader = (type, src) => {
            const s = gl.createShader(type)
            gl.shaderSource(s, src)
            gl.compileShader(s)
            if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s))
            return s
        }
        const p = gl.createProgram()
        gl.attachShader(p, shader(gl.VERTEX_SHADER, vs))
        gl.attachShader(p, shader(gl.FRAGMENT_SHADER, fs))
        gl.linkProgram(p)
        if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p))
        const uniforms = {}
        const n = gl.getProgramParameter(p, gl.ACTIVE_UNIFORMS)
        for (let i = 0; i < n; i++) {
            const name = gl.getActiveUniform(p, i).name.replace(/\[0\]$/, '')
            uniforms[name] = gl.getUniformLocation(p, name)
        }
        return {program: p, uniforms}
    }

    /** A 2D texture of one channel per corner, read with texelFetch. */
    function dataTexture(gl, internal, format, type, w, h, data) {
        const t = gl.createTexture()
        gl.bindTexture(gl.TEXTURE_2D, t)
        gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1)
        gl.texImage2D(gl.TEXTURE_2D, 0, internal, w, h, 0, format, type, data)
        for (const k of [gl.TEXTURE_MIN_FILTER, gl.TEXTURE_MAG_FILTER]) gl.texParameteri(gl.TEXTURE_2D, k, gl.NEAREST)
        for (const k of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T]) gl.texParameteri(gl.TEXTURE_2D, k, gl.CLAMP_TO_EDGE)
        return t
    }

    function arrayTexture(gl, size, count, rgba, wrap) {
        const t = gl.createTexture()
        gl.bindTexture(gl.TEXTURE_2D_ARRAY, t)
        gl.pixelStorei(gl.UNPACK_ALIGNMENT, 4)
        gl.texImage3D(gl.TEXTURE_2D_ARRAY, 0, gl.RGBA8, size, size, count, 0, gl.RGBA, gl.UNSIGNED_BYTE, rgba)
        gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR)
        gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.LINEAR)
        for (const k of [gl.TEXTURE_WRAP_S, gl.TEXTURE_WRAP_T]) gl.texParameteri(gl.TEXTURE_2D_ARRAY, k, wrap)
        gl.generateMipmap(gl.TEXTURE_2D_ARRAY)
        return t
    }

    class Terrain {
        /** @param {WebGL2RenderingContext} gl */
        constructor(gl) {
            this.gl = gl
            this.ground = program(gl, GROUND_VS, GROUND_FS)
            this.waterProgram = program(gl, WATER_VS, WATER_FS)
            this.vao = gl.createVertexArray()
            this.terrain = null
            this.layers = null
            this.first = new Uint32Array(64)
            this.water = null
            this.grid = false
            /** What it couldn't do, for the page to say. */
            this.notes = []
        }

        setGround(msg) {
            const gl = this.gl
            const max = gl.getParameter(gl.MAX_ARRAY_TEXTURE_LAYERS)
            if (msg.count > max) this.notes.push(`${msg.count} ground layers, more than this GPU's ${max}: some ground isn't drawn.`)
            if (this.layers) gl.deleteTexture(this.layers)
            this.layers = arrayTexture(gl, msg.size, Math.min(msg.count, max), msg.rgba.subarray(0, msg.size * msg.size * 4 * Math.min(msg.count, max)), gl.CLAMP_TO_EDGE)
            this.first.fill(0)
            msg.textures.forEach((t, i) => { if (i < 64) this.first[i] = t.first })
        }

        setWater(msg) {
            const gl = this.gl
            if (this.water?.frames) gl.deleteTexture(this.water.frames)
            this.water = {
                rate: msg.rate || 1,
                count: msg.count,
                colors: new Float32Array(msg.colors.flatMap(c => [c[0] / 255, c[1] / 255, c[2] / 255, c[3] / 255])),
                frames: msg.count ? arrayTexture(gl, msg.size, msg.count, msg.rgba, gl.REPEAT) : null,
            }
        }

        setTerrain(msg) {
            const gl = this.gl
            if (this.terrain) for (const t of Object.values(this.terrain.textures)) gl.deleteTexture(t)
            const {width: w, height: h} = msg
            this.terrain = {
                width: w,
                height: h,
                heights: msg.heights,
                textures: {
                    heights: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.heights),
                    ground: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.ground),
                    water: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.water),
                    corners: dataTexture(gl, gl.R8UI, gl.RED_INTEGER, gl.UNSIGNED_BYTE, w, h, msg.corners),
                    tiles: dataTexture(gl, gl.RGBA32UI, gl.RGBA_INTEGER, gl.UNSIGNED_INT, w - 1, h - 1, msg.tiles),
                },
            }
        }

        /** The ground's height at `(x, y)` (cells), between its corners. */
        heightAt(x, y) {
            const t = this.terrain
            if (!t) return 0
            const cx = Math.max(0, Math.min(t.width - 1.001, x))
            const cy = Math.max(0, Math.min(t.height - 1.001, y))
            const ix = Math.floor(cx)
            const iy = Math.floor(cy)
            const fx = cx - ix
            const fy = cy - iy
            const at = (i, j) => t.heights[j * t.width + i]
            const bottom = at(ix, iy) * (1 - fx) + at(ix + 1, iy) * fx
            const top = at(ix, iy + 1) * (1 - fx) + at(ix + 1, iy + 1) * fx
            return bottom * (1 - fy) + top * fy
        }

        get ready() {
            return !!(this.terrain && this.layers)
        }

        /** Draws it with `mvp`; `time` in seconds animates the water. */
        draw(mvp, time) {
            if (!this.ready) return
            const gl = this.gl
            const t = this.terrain
            const tiles = (t.width - 1) * (t.height - 1)
            const bind = (unit, target, texture, location) => {
                gl.activeTexture(gl.TEXTURE0 + unit)
                gl.bindTexture(target, texture)
                gl.uniform1i(location, unit)
            }
            gl.bindVertexArray(this.vao)
            gl.enable(gl.DEPTH_TEST)
            gl.disable(gl.BLEND)
            gl.depthMask(true)

            const g = this.ground
            gl.useProgram(g.program)
            gl.uniformMatrix4fv(g.uniforms.u_mvp, false, mvp)
            gl.uniform2i(g.uniforms.u_size, t.width, t.height)
            gl.uniform1uiv(g.uniforms.u_first, this.first)
            gl.uniform3fv(g.uniforms.u_light, [-0.3, -0.3, -0.25].map(v => v / Math.hypot(0.3, 0.3, 0.25)))
            gl.uniform1i(g.uniforms.u_grid, this.grid ? 1 : 0)
            bind(0, gl.TEXTURE_2D, t.textures.heights, g.uniforms.u_heights)
            bind(1, gl.TEXTURE_2D, t.textures.ground, g.uniforms.u_ground)
            bind(2, gl.TEXTURE_2D, t.textures.tiles, g.uniforms.u_tiles)
            bind(3, gl.TEXTURE_2D, t.textures.corners, g.uniforms.u_corners)
            bind(4, gl.TEXTURE_2D_ARRAY, this.layers, g.uniforms.u_layers)
            gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, tiles)

            if (!this.water) return
            const wp = this.waterProgram
            gl.useProgram(wp.program)
            gl.enable(gl.BLEND)
            gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA)
            gl.depthMask(false)
            gl.uniformMatrix4fv(wp.uniforms.u_mvp, false, mvp)
            gl.uniform2i(wp.uniforms.u_size, t.width, t.height)
            gl.uniform4fv(wp.uniforms.u_colors, this.water.colors)
            gl.uniform1i(wp.uniforms.u_textured, this.water.frames ? 1 : 0)
            gl.uniform1f(wp.uniforms.u_frame, this.water.count ? Math.floor(time * this.water.rate) % this.water.count : 0)
            bind(0, gl.TEXTURE_2D, t.textures.heights, wp.uniforms.u_heights)
            bind(1, gl.TEXTURE_2D, t.textures.water, wp.uniforms.u_water)
            bind(2, gl.TEXTURE_2D, t.textures.corners, wp.uniforms.u_corners)
            if (this.water.frames) bind(3, gl.TEXTURE_2D_ARRAY, this.water.frames, wp.uniforms.u_frames)
            gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, tiles)
            gl.depthMask(true)
        }
    }

    return {Terrain}
})()
