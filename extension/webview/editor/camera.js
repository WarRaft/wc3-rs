'use strict'

/**
 * The view's camera, HiveWE's (`camera.ixx`): it looks at a point from a
 * distance, at two angles, with a field of view of 70° (World Editor's,
 * a new camera's), turned about where it looks by its roll (0 but
 * through a camera that has one). The field of view is the game's: wider
 * than what the picture's height shows, by its 1.66 (the vertical one
 * 2 atan(tan(fov / 2) / 1.66), 45.8° for 70; as Waric reads the game). Distances are in cells
 * (128 units), as everything the editor draws, z up.
 *
 * Right drag moves it (the map follows the mouse, both ways), Ctrl +
 * right drag (or middle drag) turns it, the wheel brings it nearer or
 * further, the arrow keys and WASD move it (W and Up: forward).
 *
 * The height it looks at is a function of where it looks and how far it
 * is, never of the time (nothing goes on after a move): the game's camera
 * (`game`) the ground's there, as HiveWE's and Waric's (a cliff a jump);
 * the developer's the ground seen at its scale, the heights convolved
 * with a Gaussian of deviation `distance / SMOOTH` (`smoothHeightAt`): a
 * cliff or a hollow a slope as wide as a part of the view, the ground
 * itself where it's flat; and never so low that the camera itself would
 * be under the ground. A height given (a thing looked at) is kept till
 * the camera's moved.
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

    /** The developer's camera's scale: the deviation of its ground's Gaussian, a part of its distance. */
    const SMOOTH = 8

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
            /** The ground's height seen at a scale (cells): `heightAt` convolved with a Gaussian of that deviation. */
            this.smoothHeightAt = (x, y) => this.heightAt(x, y)
            /** Whether it's the game's camera (on the ground under where it looks); else the developer's. */
            this.game = false
            /** Whether the height it looks at was given (a thing looked at): kept till it's moved. */
            this.pinned = false
        }

        /** Looks at the middle of a map `w` by `h` cells, all of it in view, at 70°. */
        reset(w, h) {
            this.position = [w / 2, h / 2, 0]
            this.horizontal = 0
            this.vertical = -0.977
            this.fov = 70
            // As much of it in view as HiveWE's 50° high showed at 0.9 of its size.
            this.distance = Math.min(Math.max(w, h) * 0.9 * Math.tan(25 * Math.PI / 180) / Math.tan(this.verticalFov() * Math.PI / 360), 1000)
            this.roll = 0
            this.position[2] = this.heightAt(this.position[0], this.position[1])
        }

        /** The picture's vertical field of view (degrees), from the game's. */
        verticalFov() {
            return 2 * Math.atan(Math.tan(this.fov * Math.PI / 360) / 1.66) * 180 / Math.PI
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

        /**
         * Moves where it looks: `dx` to the right, `dy` toward the bottom of
         * the picture (back; `forward` is the picture's down on the ground).
         */
        move(dx, dy) {
            const {x, forward} = this.axes()
            const k = this.distance / 30
            this.position[0] += (x[0] * dx + forward[0] * dy) * k
            this.position[1] += (x[1] * dx + forward[1] * dy) * k
            this.pinned = false
        }

        /** The height it looks at, from where it looks and how far it is (see above). */
        update() {
            if (this.pinned) return
            const [x, y] = this.position
            if (this.game) {
                this.position[2] = this.heightAt(x, y)
                return
            }
            const d = this.direction
            // The camera's own place doesn't hang on the height: above the ground there by half a cell.
            const ex = x - d[0] * this.distance
            const ey = y - d[1] * this.distance
            const above = this.heightAt(ex, ey) + 0.5 + d[2] * this.distance
            this.position[2] = Math.max(this.smoothHeightAt(x, y, this.distance / SMOOTH), above)
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
            const t = Math.tan(this.verticalFov() * Math.PI / 360)
            const dir = norm([0, 1, 2].map(k => d[k] + x[k] * nx * t * aspect + y[k] * ny * t))
            return {origin: this.eye(), dir}
        }

        /** The projection times the view, for a picture `aspect` wide. */
        matrix(aspect) {
            const eye = this.eye()
            const projection = mat4.perspective(this.verticalFov() * Math.PI / 180, aspect, 0.05, 2000)
            return mat4.multiply(projection, mat4.lookAt(eye, this.position, this.basis().up))
        }
    }

    return {Camera, mat4}
})()
