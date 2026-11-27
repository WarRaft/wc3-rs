'use strict'

/**
 * The map's light, as the editor's process baked it (`editor/src/light.rs`):
 * the ground's and the things' through the day (the sun's direction, its
 * colour, the ambient light, every half an hour from midnight), the
 * shadows (`war3map.shd`), the fog (the map's options). The page picks
 * the time (noon, or the day going round as the game's, eight minutes a
 * day) and gives every shader the same few uniforms (`GLSL`).
 */
const EditorLight = (() => {
    /** What the terrain's and the things' shaders put in: light, fog. */
    const GLSL = `
uniform bool u_daylight;
uniform vec3 u_sun;
uniform vec3 u_diffuse;
uniform vec3 u_ambient;
uniform bool u_fog;
uniform float u_fogStart;
uniform float u_fogEnd;
uniform vec3 u_fogColor;
// The day's light on a surface of normal n: the ambient and the sun's;
// without a day and night model, the view own light (own).
vec3 lit(vec3 c, vec3 n, float own) {
    if (!u_daylight) return c * own;
    return c * (u_ambient + u_diffuse * max(dot(-u_sun, normalize(n)), 0.0));
}
// The fog at a distance (the game's units) from the camera.
vec3 fogged(vec3 c, float dist) {
    if (!u_fog || u_fogEnd <= u_fogStart) return c;
    return mix(c, u_fogColor, clamp((dist - u_fogStart) / (u_fogEnd - u_fogStart), 0.0, 1.0));
}
`
    /** `light::SAMPLE`, `light::SAMPLES`. */
    const SAMPLE = 10
    const SAMPLES = 48
    /** Seconds a day in the game. */
    const DAY = 480

    class Light {
        constructor() {
            this.terrain = new Float32Array(0)
            this.units = new Float32Array(0)
            this.fog = null
            this.hours = 12
            /** When the day started going round from `hours` (seconds). */
            this.from = 0
            /** Whether it was going round at the last `now`. */
            this.cycling = false
            this.showFog = false
        }

        set(msg) {
            this.terrain = msg.terrain
            this.units = msg.units
            this.fog = msg.fog
        }

        /**
         * The hours of the day now: `hours`, or going round from `hours`
         * since it started to (or was last set); stopped, it stays where
         * it went round to.
         */
        now(time, cycle) {
            if (cycle !== this.cycling) {
                if (cycle) this.from = time
                else this.hours = (this.hours + (time - this.from) / DAY * 24) % 24
                this.cycling = cycle
            }
            return cycle ? (this.hours + (time - this.from) / DAY * 24) % 24 : this.hours
        }

        /** The time of day set to `hours` at `time`: going round, from there. */
        setHours(hours, time) {
            this.hours = hours
            this.from = time
        }

        /** The light of `samples` at `hours`, between the half hours. */
        at(samples, hours) {
            if (!samples.length) return null
            const x = hours / 24 * SAMPLES
            const i = Math.floor(x) % SAMPLES
            const j = (i + 1) % SAMPLES
            const t = x - Math.floor(x)
            const v = k => samples[i * SAMPLE + k] * (1 - t) + samples[j * SAMPLE + k] * t
            return {sun: [v(0), v(1), v(2)], diffuse: [v(3), v(4), v(5)], ambient: [v(6), v(7), v(8)]}
        }

        /**
         * Gives program `u` (its uniforms by name) the light of
         * `which` ('terrain' or 'units') at `hours`, and the fog.
         */
        apply(gl, u, which, hours) {
            const l = this.at(which === 'units' ? this.units : this.terrain, hours)
            gl.uniform1i(u.u_daylight, l ? 1 : 0)
            if (l) {
                gl.uniform3fv(u.u_sun, l.sun)
                gl.uniform3fv(u.u_diffuse, l.diffuse)
                gl.uniform3fv(u.u_ambient, l.ambient)
            }
            const fog = this.showFog && this.fog
            gl.uniform1i(u.u_fog, fog ? 1 : 0)
            if (fog) {
                gl.uniform1f(u.u_fogStart, fog.start)
                gl.uniform1f(u.u_fogEnd, fog.end)
                gl.uniform3fv(u.u_fogColor, fog.color.slice(0, 3).map(v => v / 255))
            }
        }
    }

    return {Light, GLSL}
})()
