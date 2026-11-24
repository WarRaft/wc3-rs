/**
 * A look, not a test (`E2E_SUITE=probe.js`): opens each of `E2E_OPEN`
 * (URIs, `|`-separated; `{map}` the map's path) as the Explorer would
 * (`vscode.open`), each after the notifications are cleared, and takes a
 * screenshot after each.
 */
const vscode = require('vscode')
const fs = require('fs')
const path = require('path')

const out = process.env.E2E_OUT
const log = line => fs.appendFileSync(path.join(out, 'log.txt'), line + '\n')
const sleep = ms => new Promise(r => setTimeout(r, ms))

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
    await ext.activate()
    const list = (process.env.E2E_OPEN || '').split('|').filter(Boolean)
    for (const [k, raw] of list.entries()) {
        await vscode.commands.executeCommand('workbench.action.closeAllEditors')
        await vscode.commands.executeCommand('notifications.clearAll')
        const uri = vscode.Uri.parse(raw.replace('{map}', process.env.E2E_MAP))
        log(`open ${uri.toString()}`)
        await vscode.commands.executeCommand('vscode.open', uri).then(undefined, e => log(`  failed: ${e.message}`))
        await shot(`probe-${k + 1}`)
        for (const tab of vscode.window.tabGroups.all.flatMap(g => g.tabs)) log(`  tab ${tab.label}: ${tab.input?.viewType ?? tab.input?.uri?.scheme ?? ''}`)
    }
}
