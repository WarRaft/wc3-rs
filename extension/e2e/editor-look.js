/**
 * A look, not a test (`E2E_SUITE=editor-look.js`): the project's World
 * Editor opened, its map drawn, what it drew said, and screenshots: all
 * of it, then close up over the first thing of each model whose path has
 * one of `E2E_LOOK` (`|`-separated; a footman and the cliffs by default).
 * For the game as another project has it (`E2E_HD=1`: Reforged's HD).
 */
const vscode = require('vscode')
const fs = require('fs')
const path = require('path')

const out = process.env.E2E_OUT
const log = line => fs.appendFileSync(path.join(out, 'log.txt'), line + '\n')
const sleep = ms => new Promise(r => setTimeout(r, ms))

async function until(what, why, ms = 60000) {
    const end = Date.now() + ms
    for (;;) {
        const v = await what()
        if (v) return v
        if (Date.now() > end) throw new Error(`timed out: ${why}`)
        await sleep(100)
    }
}

async function shot(name) {
    await sleep(1500)
    const done = path.join(out, 'shot.done')
    fs.rmSync(done, {force: true})
    fs.writeFileSync(path.join(out, 'shot.request'), path.join(out, `${name}.png`))
    const end = Date.now() + 10000
    while (!fs.existsSync(done) && Date.now() < end) await sleep(100)
    log(`  screenshot ${name}.png`)
}

module.exports.run = async () => {
    const ext = vscode.extensions.all.find(e => e.packageJSON.name === 'wc3' && e.packageJSON.publisher === 'WarRaft')
    const api = await ext.activate()
    const manifest = vscode.Uri.file(path.join(path.dirname(process.env.E2E_MAP), 'wc3.bni'))
    const we = await vscode.commands.executeCommand('wc3.editor.open', manifest, {groupId: 1})
    const drawn = await until(() => we.drawn || we.error, 'the map drawn', 180000)
    log(`drawn: ${JSON.stringify(drawn)}${we.error ? `; error ${we.error}` : ''}`)
    await vscode.commands.executeCommand('notifications.clearAll')
    await sleep(3000)
    await shot('look-all')
    for (const [k, what] of (process.env.E2E_LOOK || 'Footman|Grave|Tree').split('|').entries()) {
        we.focused = undefined
        await we.panel.webview.postMessage({type: 'focusEffect', path: what, distance: 6})
        const found = await until(() => we.focused !== undefined, 'focused', 20000).then(() => we.focused)
        log(`look at ${what}: ${found || 'none'}`)
        if (found) await shot(`look-${k + 1}-${what.replace(/\W/g, '')}`)
        await we.panel.webview.postMessage({type: 'unfocus'})
    }
    void api
}
