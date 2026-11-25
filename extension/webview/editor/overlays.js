'use strict'

/**
 * The lines over the map, as the editor's process worked them out (see
 * `editor/src/overlays.rs`): the bounds (the camera's, the playable
 * area) and the cameras, each point on the ground under it.
 */
const EditorOverlays = (() => {
    const VS = `#version 300 es
precision highp float;
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec4 a_color;
uniform mat4 u_mvp;
out vec4 v_color;
void main() {
    v_color = a_color;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}`
    const FS = `#version 300 es
precision highp float;
in vec4 v_color;
out vec4 color;
void main() { color = v_color; }`
    /** Floats a point has (`overlays::POINT`). */
    const POINT = 7

    class Lines {
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
            this.program = gl.createProgram()
            gl.attachShader(this.program, shader(gl.VERTEX_SHADER, VS))
            gl.attachShader(this.program, shader(gl.FRAGMENT_SHADER, FS))
            gl.linkProgram(this.program)
            this.u_mvp = gl.getUniformLocation(this.program, 'u_mvp')
            /** Each set of lines by name: its vertex array and point count. */
            this.sets = {}
        }

        set(name, points) {
            const gl = this.gl
            const old = this.sets[name]
            if (old) {
                gl.deleteVertexArray(old.vao)
                gl.deleteBuffer(old.buffer)
            }
            const vao = gl.createVertexArray()
            gl.bindVertexArray(vao)
            const buffer = gl.createBuffer()
            gl.bindBuffer(gl.ARRAY_BUFFER, buffer)
            gl.bufferData(gl.ARRAY_BUFFER, points, gl.STATIC_DRAW)
            gl.enableVertexAttribArray(0)
            gl.vertexAttribPointer(0, 3, gl.FLOAT, false, POINT * 4, 0)
            gl.enableVertexAttribArray(1)
            gl.vertexAttribPointer(1, 4, gl.FLOAT, false, POINT * 4, 12)
            gl.bindVertexArray(null)
            this.sets[name] = {vao, buffer, count: points.length / POINT}
        }

        /** Draws the sets `shown` names. */
        draw(mvp, shown) {
            const gl = this.gl
            gl.useProgram(this.program)
            gl.uniformMatrix4fv(this.u_mvp, false, mvp)
            gl.enable(gl.DEPTH_TEST)
            gl.disable(gl.BLEND)
            for (const [name, set] of Object.entries(this.sets)) {
                if (!shown[name] || !set.count) continue
                gl.bindVertexArray(set.vao)
                gl.drawArrays(gl.LINES, 0, set.count)
            }
            gl.bindVertexArray(null)
        }
    }

    return {Lines}
})()
