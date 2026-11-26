'use strict'

/**
 * Draws a terrain as the editor's process worked it out (see
 * `editor/src/terrain.rs`): no tile is worked out here. The ground is a
 * quad per tile, its corners' heights fetched from a texture; its colour
 * the tile's up to four ground layers, each over the one before through
 * its blend mask (HiveWE's `terrain.frag`), lit by its ground's normal.
 * No ground under a cliff or a ramp: their models (the process placed
 * them) stand there, each vertex lifted by the ground under it (HiveWE's
 * `cliff.vert`), in its cliff type's texture.
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
uniform usampler2D u_tileFlags;
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
    // No ground under a cliff or a ramp, but at a ramp's foot
    // (terrain::tile::has_ground).
    uint tf = texelFetch(u_tileFlags, tile, 0).r;
    bool ground = (tf & 5u) == 0u || (tf & 2u) != 0u;
    gl_Position = ground ? u_mvp * vec4(vec2(c), z, 1.0) : vec4(2.0, 0.0, 0.0, 1.0);
}`

    const GROUND_FS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2DArray;
uniform sampler2DArray u_layers;
uniform uint u_first[64];
uniform vec3 u_light;
uniform bool u_grid;
uniform vec3 u_brush;
uniform highp usampler2D u_pathing;
uniform sampler2D u_regions;
uniform bool u_showPathing;
uniform bool u_showRegions;
// The pathing's colours as the game's pathing textures draw them (red no
// walking, green no flying, blue no building), half over; the regions'
// picture over that.
vec3 overlays(vec3 c, vec2 pos) {
    ivec2 cell = ivec2(floor(pos * 4.0));
    if (u_showPathing) {
        ivec2 size = textureSize(u_pathing, 0);
        if (all(greaterThanEqual(cell, ivec2(0))) && all(lessThan(cell, size))) {
            uint f = texelFetch(u_pathing, cell, 0).r;
            vec3 p = vec3((f & 2u) >> 1, (f & 4u) >> 2, (f & 8u) >> 3);
            if ((f & 14u) != 0u) c = mix(c, p, 0.5);
        }
    }
    if (u_showRegions) {
        ivec2 size = textureSize(u_regions, 0);
        if (all(greaterThanEqual(cell, ivec2(0))) && all(lessThan(cell, size))) {
            vec4 r = texelFetch(u_regions, cell, 0);
            c = mix(c, r.rgb / max(r.a, 0.001), r.a);
        }
    }
    return c;
}
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
    color.rgb = overlays(color.rgb, v_pos);
    // The brush: a ring where it would be laid.
    if (u_brush.z > 0.0) {
        float d = abs(length(v_pos - u_brush.xy) - u_brush.z) / fwidth(length(v_pos - u_brush.xy));
        color.rgb = mix(color.rgb, vec3(0.2, 1.0, 0.4), (1.0 - clamp(d - 0.5, 0.0, 1.0)) * 0.9);
    }
    if (u_grid) {
        vec2 g = abs(fract(v_pos - 0.5) - 0.5) / fwidth(v_pos);
        float line = 1.0 - clamp(min(g.x, g.y), 0.0, 1.0);
        color.rgb = mix(color.rgb, vec3(1.0), line * 0.25);
    }
    color.a = 1.0;
}`

    const CLIFF_VS = `#version 300 es
precision highp float;
precision highp int;
precision highp sampler2D;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec2 a_uv;
layout(location = 2) in vec3 a_normal;
layout(location = 3) in vec4 a_inst;
uniform mat4 u_mvp;
uniform ivec2 u_size;
uniform sampler2D u_ground;
out vec3 v_uv;
out vec3 v_normal;
out vec2 v_pos;
float ground(ivec2 c) { return texelFetch(u_ground, clamp(c, ivec2(0), u_size - 1), 0).r; }
void main() {
    // The models lie a quarter turn round (HiveWE's cliff.vert).
    vec3 p = vec3(a_pos.y, -a_pos.x, a_pos.z) / 128.0 + a_inst.xyz;
    ivec2 c = ivec2(p.xy);
    vec3 terrain = normalize(vec3(ground(c - ivec2(1, 0)) - ground(c + ivec2(1, 0)), ground(c - ivec2(0, 1)) - ground(c + ivec2(0, 1)), 2.0));
    vec3 n = vec3(a_normal.y, -a_normal.x, a_normal.z);
    v_normal = normalize(vec3(n.xy + terrain.xy, n.z * terrain.z));
    v_uv = vec3(a_uv, a_inst.w);
    v_pos = p.xy;
    gl_Position = u_mvp * vec4(p.xy, p.z + ground(c), 1.0);
}`

    const CLIFF_FS = `#version 300 es
precision highp float;
precision highp sampler2DArray;
uniform sampler2DArray u_cliffs;
uniform vec3 u_light;
uniform highp usampler2D u_pathing;
uniform sampler2D u_regions;
uniform bool u_showPathing;
uniform bool u_showRegions;
// The pathing's colours as the game's pathing textures draw them (red no
// walking, green no flying, blue no building), half over; the regions'
// picture over that.
vec3 overlays(vec3 c, vec2 pos) {
    ivec2 cell = ivec2(floor(pos * 4.0));
    if (u_showPathing) {
        ivec2 size = textureSize(u_pathing, 0);
        if (all(greaterThanEqual(cell, ivec2(0))) && all(lessThan(cell, size))) {
            uint f = texelFetch(u_pathing, cell, 0).r;
            vec3 p = vec3((f & 2u) >> 1, (f & 4u) >> 2, (f & 8u) >> 3);
            if ((f & 14u) != 0u) c = mix(c, p, 0.5);
        }
    }
    if (u_showRegions) {
        ivec2 size = textureSize(u_regions, 0);
        if (all(greaterThanEqual(cell, ivec2(0))) && all(lessThan(cell, size))) {
            vec4 r = texelFetch(u_regions, cell, 0);
            c = mix(c, r.rgb / max(r.a, 0.001), r.a);
        }
    }
    return c;
}
in vec3 v_uv;
in vec3 v_normal;
in vec2 v_pos;
out vec4 color;
void main() {
    color = texture(u_cliffs, v_uv);
    color.rgb *= clamp((dot(-u_light, normalize(v_normal)) + 1.0) * 0.5, 0.0, 1.0);
    color.rgb = overlays(color.rgb, v_pos);
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

    /** The light's direction, the ground's and the cliffs'. */
    const LIGHT = [-0.3, -0.3, -0.25].map(v => v / Math.hypot(0.3, 0.3, 0.25))

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
            this.cliffProgram = program(gl, CLIFF_VS, CLIFF_FS)
            /** Each cliff model's vertex array, index count and instances. */
            this.cliffs = []
            this.cliffTextures = null
            /** The pathing's and the regions' pictures, and which show. */
            this.pathing = dataTexture(gl, gl.R8UI, gl.RED_INTEGER, gl.UNSIGNED_BYTE, 1, 1, new Uint8Array(1))
            this.regions = dataTexture(gl, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, 1, 1, new Uint8Array(4))
            this.show = {pathing: false, regions: false}
            /** The brush's ring: x, y, radius (cells); radius 0 none. */
            this.brush = [0, 0, 0]
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

        setCliffs(msg) {
            const gl = this.gl
            for (const c of this.cliffs) {
                gl.deleteVertexArray(c.vao)
                c.buffers.forEach(b => gl.deleteBuffer(b))
            }
            // No textures (after an edit): the ones it has.
            if (msg.count) {
                if (this.cliffTextures) gl.deleteTexture(this.cliffTextures)
                this.cliffTextures = arrayTexture(gl, msg.size, msg.count, msg.rgba, gl.REPEAT)
                this.cliffTextureNotes = msg.textures.filter(t => t.error).map(t => `Cliff texture ${t.error}`)
            }
            this.cliffs = []
            this.cliffNotes = [...(this.cliffTextureNotes ?? []), ...msg.notes]
            for (const m of msg.meshes) {
                if (m.error) {
                    this.cliffNotes.push(m.error)
                    continue
                }
                if (!m.instances.length || !m.indices.length) continue
                const vao = gl.createVertexArray()
                gl.bindVertexArray(vao)
                const buffers = []
                const attribute = (location, data, size, divisor) => {
                    const b = gl.createBuffer()
                    buffers.push(b)
                    gl.bindBuffer(gl.ARRAY_BUFFER, b)
                    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW)
                    gl.enableVertexAttribArray(location)
                    gl.vertexAttribPointer(location, size, gl.FLOAT, false, 0, 0)
                    gl.vertexAttribDivisor(location, divisor)
                }
                attribute(0, m.positions, 3, 0)
                attribute(1, m.uvs, 2, 0)
                attribute(2, m.normals, 3, 0)
                attribute(3, m.instances, 4, 1)
                const ib = gl.createBuffer()
                buffers.push(ib)
                gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, ib)
                gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, m.indices, gl.STATIC_DRAW)
                gl.bindVertexArray(null)
                this.cliffs.push({vao, buffers, count: m.indices.length, instances: m.instances.length / 4})
            }
        }

        setOverlays(msg) {
            const gl = this.gl
            gl.deleteTexture(this.pathing)
            gl.deleteTexture(this.regions)
            const p = msg.pathing.width ? msg.pathing : {width: 1, height: 1, data: new Uint8Array(1)}
            const r = msg.regionPicture.width ? msg.regionPicture : {width: 1, height: 1, data: new Uint8Array(4)}
            this.pathing = dataTexture(gl, gl.R8UI, gl.RED_INTEGER, gl.UNSIGNED_BYTE, p.width, p.height, p.data)
            this.regions = dataTexture(gl, gl.RGBA8, gl.RGBA, gl.UNSIGNED_BYTE, r.width, r.height, r.data)
        }

        /** Binds the overlays' pictures (units 6 and 7) for program `p`. */
        bindOverlays(p) {
            const gl = this.gl
            gl.activeTexture(gl.TEXTURE6)
            gl.bindTexture(gl.TEXTURE_2D, this.pathing)
            gl.uniform1i(p.uniforms.u_pathing, 6)
            gl.activeTexture(gl.TEXTURE7)
            gl.bindTexture(gl.TEXTURE_2D, this.regions)
            gl.uniform1i(p.uniforms.u_regions, 7)
            gl.uniform1i(p.uniforms.u_showPathing, this.show.pathing ? 1 : 0)
            gl.uniform1i(p.uniforms.u_showRegions, this.show.regions ? 1 : 0)
        }

        setTerrain(msg) {
            const gl = this.gl
            if (this.terrain) for (const t of Object.values(this.terrain.textures)) gl.deleteTexture(t)
            const {width: w, height: h} = msg
            this.terrain = {
                width: w,
                height: h,
                offset: msg.offset,
                heights: msg.heights,
                textures: {
                    heights: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.heights),
                    ground: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.ground),
                    water: dataTexture(gl, gl.R32F, gl.RED, gl.FLOAT, w, h, msg.water),
                    corners: dataTexture(gl, gl.R8UI, gl.RED_INTEGER, gl.UNSIGNED_BYTE, w, h, msg.corners),
                    tiles: dataTexture(gl, gl.RGBA32UI, gl.RGBA_INTEGER, gl.UNSIGNED_INT, w - 1, h - 1, msg.tiles),
                    tileFlags: dataTexture(gl, gl.R8UI, gl.RED_INTEGER, gl.UNSIGNED_BYTE, w - 1, h - 1, msg.tileFlags),
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

        /** Where the ray from `origin` along `dir` meets the ground, for the brush's ring. */
        groundHit(origin, dir) {
            const t = this.terrain
            if (!t) return null
            for (let s = 0; s < 3000; s += 0.1) {
                const p = [origin[0] + dir[0] * s, origin[1] + dir[1] * s, origin[2] + dir[2] * s]
                const inside = p[0] >= 0 && p[1] >= 0 && p[0] <= t.width - 1 && p[1] <= t.height - 1
                if (inside && p[2] <= this.heightAt(p[0], p[1])) return p
            }
            return null
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
            gl.uniform3fv(g.uniforms.u_light, LIGHT)
            gl.uniform1i(g.uniforms.u_grid, this.grid ? 1 : 0)
            gl.uniform3fv(g.uniforms.u_brush, this.brush)
            bind(0, gl.TEXTURE_2D, t.textures.heights, g.uniforms.u_heights)
            bind(1, gl.TEXTURE_2D, t.textures.ground, g.uniforms.u_ground)
            bind(2, gl.TEXTURE_2D, t.textures.tiles, g.uniforms.u_tiles)
            bind(3, gl.TEXTURE_2D, t.textures.corners, g.uniforms.u_corners)
            bind(4, gl.TEXTURE_2D_ARRAY, this.layers, g.uniforms.u_layers)
            bind(5, gl.TEXTURE_2D, t.textures.tileFlags, g.uniforms.u_tileFlags)
            this.bindOverlays(g)
            gl.drawArraysInstanced(gl.TRIANGLES, 0, 6, tiles)

            if (this.cliffs.length && this.cliffTextures) {
                const c = this.cliffProgram
                gl.useProgram(c.program)
                gl.uniformMatrix4fv(c.uniforms.u_mvp, false, mvp)
                gl.uniform2i(c.uniforms.u_size, t.width, t.height)
                gl.uniform3fv(c.uniforms.u_light, LIGHT)
                bind(0, gl.TEXTURE_2D, t.textures.ground, c.uniforms.u_ground)
                bind(1, gl.TEXTURE_2D_ARRAY, this.cliffTextures, c.uniforms.u_cliffs)
                this.bindOverlays(c)
                for (const m of this.cliffs) {
                    gl.bindVertexArray(m.vao)
                    gl.drawElementsInstanced(gl.TRIANGLES, m.count, gl.UNSIGNED_SHORT, 0, m.instances)
                }
                gl.bindVertexArray(this.vao)
            }

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
