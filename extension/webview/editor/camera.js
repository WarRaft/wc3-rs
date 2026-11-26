'use strict'

/**
 * The view's camera, HiveWE's (`camera.ixx`): it looks at a point from a
 * distance, at two angles, with a field of view of 70° (World Editor's,
 * a new camera's; HiveWE's is 50°), turned about where it looks by its
 * roll (0 but through a camera that has one). Distances are in cells
 * (128 units), as everything the editor draws, z up.
 *
 * Right drag moves it, Ctrl + right drag (or middle drag) turns it, the
 * wheel brings it nearer or further, the arrow keys and WASD move it.
 */
const EditorCamera = (() => {
    const mat4 = {
        perspective(fovy, aspect, near, far) {
            const f = 1 / Math.tan(fovy / 2)
            const nf = 1 / (near - far)
            return new Float32Array([f / aspect, 0, 0, 0, 0, f, 0, 0, 0, 0, (far + near) * nf, -1, 0, 0, 2 * far * near * nf, 0])
        },
        lookAt(eye, target, up) {
            const z = norm(sub(eye, target))
            const x = norm(cross(up, z))
            const y = cross(z, x)
            return new Float32Array([
                x[0], y[0], z[0], 0,
                x[1], y[1], z[1], 0,
                x[2], y[2], z[2], 0,
                -dot(x, eye), -dot(y, eye), -dot(z, eye), 1,
            ])
        },
        multiply(a, b) {
            const out = new Float32Array(16)
            for (let c = 0; c < 4; c++) {
                for (let r = 0; r < 4; r++) {
                    let s = 0
                    for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k]
                    out[c * 4 + r] = s
                }
            }
            return out
        },
    }
    const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
    const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
    const norm = a => {
        const l = Math.hypot(a[0], a[1], a[2]) || 1
        return [a[0] / l, a[1] / l, a[2] / l]
    }

    class Camera {
        constructor() {
            this.position = [0, 0, 0]
            this.distance = 20
            this.horizontal = 0
            this.vertical = -0.977
            this.fov = 70
            /** Radians about the view's direction: the picture turned clockwise. */
            this.roll = 0
            /** The ground's height under a point, to keep the camera on it. */
            this.heightAt = () => 0
        }

        /** Looks at the middle of a map `w` by `h` cells, all of it in view, at 70°. */
        reset(w, h) {
            this.position = [w / 2, h / 2, 0]
            this.horizontal = 0
            this.vertical = -0.977
            this.distance = Math.min(Math.max(w, h) * 0.9, 1000)
            this.fov = 70
            this.roll = 0
            this.position[2] = this.heightAt(this.position[0], this.position[1])
        }

        get direction() {
            const v = this.vertical
            const h = this.horizontal
            return norm([Math.cos(v) * Math.sin(h), Math.cos(v) * Math.cos(h), Math.sin(v)])
        }

        /** The camera's right and forward on the ground. */
        axes() {
            const x = norm(cross(this.direction, [0, 0, 1]))
            const forward = cross(x, [0, 0, 1])
            return {x, forward}
        }

        /** The picture's right and up in the world, turned by the roll. */
        basis() {
            const d = this.direction
            const {x} = this.axes()
            const y = cross(x, d)
            const c = Math.cos(this.roll)
            const s = Math.sin(this.roll)
            return {
                right: [0, 1, 2].map(k => x[k] * c + y[k] * s),
                up: [0, 1, 2].map(k => y[k] * c - x[k] * s),
            }
        }

        move(dx, dy) {
            const {x, forward} = this.axes()
            const k = this.distance / 30
            this.position[0] += (x[0] * dx - forward[0] * dy) * k
            this.position[1] += (x[1] * dx - forward[1] * dy) * k
            this.position[2] = this.heightAt(this.position[0], this.position[1])
        }

        turn(dx, dy) {
            this.horizontal += dx * 0.0025
            this.vertical = Math.max(-Math.PI / 2 + 0.001, Math.min(Math.PI / 2 - 0.001, this.vertical - dy * 0.0025))
        }

        zoom(delta) {
            this.distance = Math.max(0.5, Math.min(1000, this.distance * Math.pow(0.999, -delta)))
        }

        /** Where the camera is. */
        eye() {
            const d = this.direction
            return [this.position[0] - d[0] * this.distance, this.position[1] - d[1] * this.distance, this.position[2] - d[2] * this.distance]
        }

        /**
         * The ray from the camera through the point `(nx, ny)` of the view
         * (-1 to 1, y up), for a picture `aspect` wide: its origin and
         * direction.
         */
        ray(nx, ny, aspect) {
            const d = this.direction
            const {right: x, up: y} = this.basis()
            const t = Math.tan(this.fov * Math.PI / 360)
            const dir = norm([0, 1, 2].map(k => d[k] + x[k] * nx * t * aspect + y[k] * ny * t))
            return {origin: this.eye(), dir}
        }

        /** The projection times the view, for a picture `aspect` wide. */
        matrix(aspect) {
            const eye = this.eye()
            const projection = mat4.perspective(this.fov * Math.PI / 180, aspect, 0.05, 2000)
            return mat4.multiply(projection, mat4.lookAt(eye, this.position, this.basis().up))
        }
    }

    return {Camera, mat4}
})()
