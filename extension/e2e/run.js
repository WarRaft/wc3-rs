#!/usr/bin/env node
/**
 * Runs the tests in the editor (`suite.js`) in a VS Code of its own: the
 * one installed (`VSCODE`, else /Applications's), a profile and
 * extensions folder of its own (no other extension), this extension
 * from its folder, on a copy of a map in a folder of its own with a
 * `wc3.bni` naming the game (`WC3_GAME`).
 *
 *   node e2e/run.js [map.w3x]
 *
 * What the tests say goes to `<out>/log.txt`, printed when it's done;
 * screenshots to `<out>/*.png` (macOS `screencapture`: the terminal needs
 * Screen Recording). `E2E_OUT` is `<out>`, a fresh temporary folder
 * without it. Exits as the tests did.
 */
const {spawn, execFile} = require('child_process')
const fs = require('fs')
const os = require('os')
const path = require('path')

const ext = path.resolve(__dirname, '..')
const map = path.resolve(process.argv[2] || path.join(ext, '..', '..', 'maps', 'Diablo', 'Diablo-III-beta-Unprotected.w3x'))
const game = process.env.WC3_GAME || '/Volumes/TrascendSSD/Warcraft III/Warcraft III 1.26a Full (EN)'
const code = process.env.VSCODE || '/Applications/Visual Studio Code.app/Contents/MacOS/Code'
const out = process.env.E2E_OUT || fs.mkdtempSync(path.join(os.tmpdir(), 'wc3-e2e-'))

fs.rmSync(path.join(out, 'workspace'), {recursive: true, force: true})
const workspace = path.join(out, 'workspace')
fs.mkdirSync(workspace, {recursive: true})
fs.copyFileSync(map, path.join(workspace, 'map.w3x'))
fs.writeFileSync(path.join(workspace, 'wc3.bni'), `[project]\ngame=${JSON.stringify(game)}\n`)
fs.rmSync(path.join(out, 'log.txt'), {force: true})

// The profile's socket path must be short (macOS: 103 characters).
const profile = fs.mkdtempSync(path.join(os.tmpdir(), 'wc3e2e-'))
// A window filling the screen, with nothing but the editor in it.
fs.mkdirSync(path.join(profile, 'u', 'User'), {recursive: true})
fs.writeFileSync(path.join(profile, 'u', 'User', 'settings.json'), JSON.stringify({
    'window.newWindowDimensions': 'maximized',
    'window.restoreWindows': 'none',
    'workbench.startupEditor': 'none',
    'workbench.secondarySideBar.defaultVisibility': 'hidden',
    'workbench.tips.enabled': false,
    'chat.disableAIFeatures': true,
    'extensions.ignoreRecommendations': true,
    'security.workspace.trust.enabled': false,
}, null, 2))
const args = [
    workspace,
    `--extensionDevelopmentPath=${ext}`,
    `--extensionTestsPath=${path.join(__dirname, process.env.E2E_SUITE || 'suite.js')}`,
    `--user-data-dir=${path.join(profile, 'u')}`,
    `--extensions-dir=${path.join(profile, 'x')}`,
    '--disable-extensions',
    '--disable-workspace-trust',
    '--skip-welcome',
    '--skip-release-notes',
    '--new-window',
]
console.log(`VS Code on ${workspace} (out: ${out})`)
const child = spawn(code, args, {env: {...process.env, E2E_OUT: out, E2E_MAP: path.join(workspace, 'map.w3x'), ELECTRON_RUN_AS_NODE: ''}, stdio: ['ignore', 'pipe', 'pipe']})
child.stdout.on('data', d => process.env.E2E_VERBOSE && process.stderr.write(d))
child.stderr.on('data', d => process.env.E2E_VERBOSE && process.stderr.write(d))
// The screenshots the tests ask for (`shot.request`: the file to write).
const request = path.join(out, 'shot.request')
const watcher = setInterval(() => {
    if (!fs.existsSync(request)) return
    const file = fs.readFileSync(request, 'utf8')
    fs.rmSync(request, {force: true})
    execFile('screencapture', ['-x', file], e => fs.writeFileSync(path.join(out, 'shot.done'), e ? (e.message.split('\n').slice(1).join(' ') || e.message) : ''))
}, 100)
child.on('exit', status => {
    clearInterval(watcher)
    fs.rmSync(profile, {recursive: true, force: true})
    const log = path.join(out, 'log.txt')
    if (fs.existsSync(log)) process.stdout.write(fs.readFileSync(log, 'utf8'))
    const shots = fs.readdirSync(out).filter(f => f.endsWith('.png')).map(f => path.join(out, f))
    if (shots.length) console.log(`screenshots:\n  ${shots.join('\n  ')}`)
    console.log(status === 0 ? 'passed' : `failed (${status})`)
    process.exit(status ?? 1)
})
