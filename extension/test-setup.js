'use strict'

/**
 * Test Map's setup where the game can't run by itself (`wc3.testMapSetup`).
 *
 * macOS: a window of steps that does the work, the user only clicking:
 * Porting Kit (found, or its page opened, then found when it's there); a
 * wrapper of its (an `.app` with a `drive_c`: in `/Applications`,
 * `~/Applications` and a level under them); the game in it (found, or the
 * project's game folder copied in with a click); the wrapper's `wine`;
 * then `[test]` written into `wc3.bni` (`game=` the wrapper's game folder,
 * `command=` its wine with its prefix running `War3.exe -loadfile`), and
 * Test Map run. Each step checked again on every click and every few
 * seconds while the window is open.
 *
 * Linux: a window that says no way has been found yet, and what to write
 * by hand.
 */

const fs = require('fs')
const os = require('os')
const path = require('path')
const {window, workspace, commands, env, Uri, ViewColumn, WorkspaceEdit, Range} = require('vscode')

const PORTING_KIT_PAGE = 'https://www.portingkit.com/'

/** Where apps and wrappers are looked for (`WC3_SETUP_DIRS`, `:` between: the tests'). */
const appDirs = () => (process.env.WC3_SETUP_DIRS ? process.env.WC3_SETUP_DIRS.split(':') : ['/Applications', path.join(os.homedir(), 'Applications')])

const isDir = p => {
    try {
        return fs.statSync(p).isDirectory()
    } catch {
        return false
    }
}

/** The `.app`s in `dir` and a level under it. */
function apps(dir) {
    const out = []
    let names = []
    try {
        names = fs.readdirSync(dir)
    } catch {
        return out
    }
    for (const n of names) {
        const p = path.join(dir, n)
        if (n.endsWith('.app')) out.push(p)
        else if (isDir(p) && !n.startsWith('.')) {
            try {
                for (const m of fs.readdirSync(p)) if (m.endsWith('.app')) out.push(path.join(p, m))
            } catch {}
        }
    }
    return out
}

/** A file named one of `names` (any case) under `dir`, `depth` levels down at most. */
function findFile(dir, names, depth) {
    let entries = []
    try {
        entries = fs.readdirSync(dir, {withFileTypes: true})
    } catch {
        return null
    }
    for (const e of entries) if (e.isFile() && names.includes(e.name.toLowerCase())) return path.join(dir, e.name)
    if (depth <= 0) return null
    for (const e of entries) {
        if (!e.isDirectory() || e.name.startsWith('.') || e.name === 'dosdevices') continue
        const found = findFile(path.join(dir, e.name), names, depth - 1)
        if (found) return found
    }
    return null
}

/**
 * A wrapper: an `.app` with a Windows `drive_c` (Porting Kit's, Wineskin's:
 * `Contents/SharedSupport/prefix`, or `Contents` in older ones), its game
 * (the folder of `War3.exe` in it) and its `wine`.
 */
function wrapperOf(app) {
    const prefix = [path.join(app, 'Contents', 'SharedSupport', 'prefix'), path.join(app, 'Contents')].find(p => isDir(path.join(p, 'drive_c')))
    if (!prefix) return null
    const exe = findFile(path.join(prefix, 'drive_c'), ['war3.exe'], 4)
    const wine = findFile(path.join(app, 'Contents'), ['wine64', 'wine'], 6)
    return {app, name: path.basename(app, '.app'), prefix, driveC: path.join(prefix, 'drive_c'), game: exe ? path.dirname(exe) : null, wine}
}

/** The project's game folders (`[project] game=`), each resolved against its root. */
function projectGames(manifest) {
    const text = fs.readFileSync(manifest, 'utf8')
    const out = []
    let section = ''
    for (const line of text.split(/\r?\n/)) {
        const head = /^\s*\[([^\]]*)\]/.exec(line)
        if (head) section = head[1].trim()
        const m = /^\s*game\s*=\s*(.+?)\s*$/.exec(line)
        if (m && section === 'project') out.push(path.resolve(path.dirname(manifest), m[1].replace(/^"|"$/g, '')))
    }
    return out
}

/** Where things stand, each step's. */
function scan(manifest, chosen) {
    const all = appDirs().flatMap(apps)
    const portingKit = all.find(a => /^porting kit/i.test(path.basename(a))) ?? null
    const wrappers = all.filter(a => a !== portingKit).map(wrapperOf).filter(Boolean)
    const wrapper = wrappers.find(w => w.app === chosen) ?? wrappers.find(w => w.game) ?? wrappers[0] ?? null
    const source = projectGames(manifest).reverse().find(g => findFile(g, ['war3.exe'], 0)) ?? null
    const text = fs.readFileSync(manifest, 'utf8')
    const written = !!wrapper?.game && text.includes(`game=${JSON.stringify(wrapper.game)}`)
    return {portingKit, wrappers, wrapper, source, written}
}

/** `wc3.bni`'s `[test]` for `w`: its game folder, its wine with its prefix. */
function testSection(w) {
    const q = s => JSON.stringify(s)
    return `[test]\ngame=${q(w.game)}\ncommand=${['/usr/bin/env', `WINEPREFIX=${w.prefix}`, w.wine, '{exe}', '-loadfile', '{map}'].map(q).join(',')}\nwindow=1\n`
}

/** `text` with its `[test]` sections out and `section` at the end. */
function withTest(text, section) {
    const lines = text.split(/\r?\n/)
    const out = []
    let inTest = false
    for (const line of lines) {
        const head = /^\s*\[([^\]]*)\]/.exec(line)
        if (head) inTest = head[1].trim() === 'test'
        if (!inTest) out.push(line)
    }
    while (out.length && out[out.length - 1].trim() === '') out.pop()
    return `${out.join('\n')}\n\n${section}`
}

const esc = s => String(s).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'})[c])

/** The window's page: the steps as `s` has them. */
function page(s, busy, said) {
    const step = (done, title, body) => `<li class="${done ? 'done' : 'todo'}"><h3>${done ? '✓' : '○'} ${title}</h3>${body}</li>`
    const button = (action, label, disabled = false) => `<button data-action="${action}"${disabled || busy ? ' disabled' : ''}>${esc(label)}</button>`
    const w = s.wrapper
    const choices = s.wrappers.length > 1
        ? `<p>${s.wrappers.map(x => `<label><input type="radio" name="w" value="${esc(x.app)}"${x === w ? ' checked' : ''}> ${esc(x.name)}${x.game ? ' (the game in it)' : ''}</label>`).join('<br>')}</p>`
        : ''
    return `<!doctype html><html><head><meta charset="utf-8"><style>
body { font-family: var(--vscode-font-family); color: var(--vscode-foreground); padding: 0 20px; }
li { margin: 14px 0; list-style: none; } h3 { margin: 0 0 4px; } .done h3 { color: var(--vscode-testing-iconPassed); }
.todo h3 { color: var(--vscode-foreground); } p { margin: 4px 0; color: var(--vscode-descriptionForeground); }
button { background: var(--vscode-button-background); color: var(--vscode-button-foreground); border: 0; padding: 4px 12px; margin: 4px 6px 0 0; cursor: pointer; }
button:disabled { opacity: .5; cursor: default; } code { font-family: var(--vscode-editor-font-family); } .said { margin-top: 16px; }
</style></head><body>
<h2>Test Map on macOS</h2>
<p>Warcraft III is a Windows game: it runs here in a Porting Kit wrapper. Each step is checked by itself; a button does what's left.</p>
<ol>
${step(!!s.portingKit, 'Porting Kit', s.portingKit ? `<p>${esc(s.portingKit)}</p>` : `<p>Download it, open the .dmg and drag Porting Kit into Applications; this step sees it then.</p>${button('download', 'Download Porting Kit')}${button('check', 'Check again')}`)}
${step(!!w, 'A wrapper', w ? `<p>${esc(w.app)}</p>${choices}` : `<p>In Porting Kit, make a port for Warcraft III (Add Game → Custom Port), or install its own; this step sees it then.</p>${button('openPortingKit', 'Open Porting Kit', !s.portingKit)}${button('check', 'Check again')}`)}
${step(!!w?.game, 'The game in it', w?.game ? `<p>${esc(w.game)}</p>` : `<p>${s.source ? `Your game (${esc(s.source)}) copied into the wrapper's C:\\Program Files\\Warcraft III.` : 'No game folder of the project has War3.exe ([project] game=).'}</p>${button('copy', busy === 'copy' ? 'Copying…' : 'Copy the game in', !w || !s.source)}`)}
${step(!!w?.wine, "The wrapper's Wine", w?.wine ? `<p>${esc(w.wine)}</p>` : `<p>${w ? 'This wrapper has no wine in it: make another.' : 'With a wrapper.'}</p>`)}
${step(s.written, 'wc3.bni', s.written ? '<p>[test] says to run the game in the wrapper.</p>' : `<p>[test] written: the wrapper's game folder and its Wine running <code>War3.exe -loadfile</code>.</p>${button('write', 'Write [test]', !w?.game || !w?.wine)}`)}
${step(false, 'Test Map', `<p>The map built and the game run with it.</p>${button('test', 'Test Map', !s.written)}`)}
</ol>
${said ? `<p class="said">${esc(said)}</p>` : ''}
<script>
const vscode = acquireVsCodeApi()
for (const b of document.querySelectorAll('button')) b.onclick = () => vscode.postMessage({action: b.dataset.action, wrapper: document.querySelector('input[name=w]:checked')?.value})
for (const r of document.querySelectorAll('input[name=w]')) r.onchange = () => vscode.postMessage({action: 'choose', wrapper: r.value})
</script></body></html>`
}

/** Linux's window: no way yet. */
function linuxPage() {
    return `<!doctype html><html><head><meta charset="utf-8"><style>body { font-family: var(--vscode-font-family); color: var(--vscode-foreground); padding: 0 20px; } code { font-family: var(--vscode-editor-font-family); }</style></head><body>
<h2>Test Map on Linux</h2>
<p>Warcraft III is a Windows game, and no way to run it here for you has been found yet.</p>
<p>If you run it yourself (Wine, Proton, Lutris …), say how in wc3.bni and Test Map uses it:</p>
<pre><code>[test]
game="/path/to/Warcraft III"
command="wine","{exe}","-loadfile","{map}"</code></pre>
</body></html>`
}

/**
 * @returns {{open: (manifest: string) => object}} the window, for the tests: its `state()` and `act(action, wrapper)`
 */
function registerTestSetup(context) {
    let current = null
    const open = manifest => {
        if (current) {
            current.panel.reveal()
            current.manifest = manifest
            current.refresh()
            return current
        }
        const panel = window.createWebviewPanel('wc3TestSetup', 'Test Map setup', ViewColumn.Active, {enableScripts: process.platform === 'darwin'})
        if (process.platform !== 'darwin') {
            panel.webview.html = linuxPage()
            current = {panel, manifest, state: () => ({platform: process.platform}), act: async () => {}, refresh: () => {}}
            panel.onDidDispose(() => (current = null))
            return current
        }
        const me = {panel, manifest, chosen: null, busy: null, said: '', last: null}
        me.state = () => me.last
        me.refresh = () => {
            me.last = scan(me.manifest, me.chosen)
            // Drawn again only when something changed (a radio picked stays).
            const html = page(me.last, me.busy, me.said)
            if (html !== me.html) panel.webview.html = me.html = html
            return me.last
        }
        me.act = async (action, wrapper) => {
            if (wrapper) me.chosen = wrapper
            const s = me.refresh()
            const w = s.wrapper
            if (action === 'download') await env.openExternal(Uri.parse(PORTING_KIT_PAGE))
            else if (action === 'openPortingKit' && s.portingKit) require('child_process').spawn('open', [s.portingKit], {detached: true, stdio: 'ignore'}).unref()
            else if (action === 'copy' && w && s.source && !me.busy) {
                me.busy = 'copy'
                me.refresh()
                const to = path.join(w.driveC, 'Program Files', 'Warcraft III')
                try {
                    await fs.promises.cp(s.source, to, {recursive: true, force: false, errorOnExist: false})
                    me.said = `Copied ${s.source} into ${to}.`
                } catch (e) {
                    me.said = `Not copied: ${e.message}`
                }
                me.busy = null
            } else if (action === 'write' && w?.game && w.wine) {
                // Its open document's changes not saved edited there (the text
                // is the truth); else the file, as it is on disk.
                const open = workspace.textDocuments.find(d => d.uri.scheme === 'file' && d.uri.fsPath === me.manifest)
                if (open?.isDirty) {
                    const edit = new WorkspaceEdit()
                    edit.replace(open.uri, new Range(0, 0, open.lineCount, 0), withTest(open.getText(), testSection(w)))
                    await workspace.applyEdit(edit)
                    await open.save()
                } else fs.writeFileSync(me.manifest, withTest(fs.readFileSync(me.manifest, 'utf8'), testSection(w)))
                me.said = 'wc3.bni: [test] written.'
            } else if (action === 'test') {
                me.said = 'Test Map…'
                me.refresh()
                const r = await commands.executeCommand('wc3.testMap', Uri.file(me.manifest))
                me.said = r?.code === 0 ? 'The game started with the map.' : `Test Map: ${(r?.output ?? '').trim().split('\n').pop()}`
            }
            return me.refresh()
        }
        panel.webview.onDidReceiveMessage(m => me.act(m.action, m.wrapper))
        // Checked again by itself: Porting Kit dragged in, a port made.
        const timer = setInterval(() => !me.busy && me.refresh(), 3000)
        panel.onDidDispose(() => {
            clearInterval(timer)
            current = null
        })
        me.refresh()
        current = me
        return me
    }
    return {open}
}

module.exports = {registerTestSetup, scan, withTest, testSection}
