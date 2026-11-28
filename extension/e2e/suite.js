/**
 * The tests in the editor, run by `run.js` in a VS Code of its own: the
 * World Editor driven as a user would (the page's messages through the
 * extension's `test` export), each step checked and written to `log.txt`.
 * Only the work in hand has steps: a step goes when its work is done.
 */
const vscode = require('vscode')
const fs = require('fs')
const path = require('path')

/**
 * The steps of the work in hand (`node e2e/run.js current`); `all` runs
 * every step.
 */
const CURRENT = ['fdf', 'fdf-links', 'toc']

const out = process.env.E2E_OUT
const map = process.env.E2E_MAP
const log = line => fs.appendFileSync(path.join(out, 'log.txt'), line + '\n')
const sleep = ms => new Promise(r => setTimeout(r, ms))

/** Waits for `what` to be truthy (and gives it back), or fails saying `why`. */
async function until(what, why, ms = 20000) {
    const end = Date.now() + ms
    for (;;) {
        const v = await what()
        if (v) return v
        if (Date.now() > end) throw new Error(`timed out: ${why}`)
        await sleep(100)
    }
}

let shots = 0
/**
 * A screenshot of the screen, named: asked of `run.js` (a file it
 * watches), so the terminal's Screen Recording is the one it needs.
 */
async function shot(name) {
    await sleep(800)
    const file = path.join(out, `${String(++shots).padStart(2, '0')}-${name}.png`)
    const done = path.join(out, 'shot.done')
    fs.rmSync(done, {force: true})
    fs.writeFileSync(path.join(out, 'shot.request'), file)
    const end = Date.now() + 10000
    while (!fs.existsSync(done) && Date.now() < end) await sleep(100)
    const said = fs.existsSync(done) ? fs.readFileSync(done, 'utf8') : 'run.js did not answer'
    log(said ? `  no screenshot: ${said.trim()}` : `  screenshot ${path.basename(file)}`)
}

function check(ok, what) {
    if (!ok) throw new Error(`failed: ${what}`)
    log(`  ok: ${what}`)
}

/**
 * The steps, in the order they run, each its name (`run.js`'s list),
 * whether it works in the World Editor (opened before the first of
 * those, closed after the last: `open`, `close`) and the steps it needs
 * run before it.
 */
async function steps(only) {
    const ext = vscode.extensions.all.find(e => e.packageJSON.name === 'wc3' && e.packageJSON.publisher === 'WarRaft')
    const api = (await ext.activate()).test
    await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    // "Extensions are disabled" (the others are, on purpose): not in the screenshots.
    await vscode.commands.executeCommand('notifications.clearAll')
    const manifest = vscode.Uri.file(path.join(path.dirname(map), 'wc3.bni'))
    const root = path.dirname(manifest.fsPath)
    let we
    const open = async () => {
        log('the World Editor opened from the button over wc3.bni: the map shown, its terrain drawn')
        we = await vscode.commands.executeCommand('wc3.editor.open', manifest, {groupId: 1})
        check(!!we && api.worldEditors.get(root) === we, 'one editor, of the project')
        // (Reforged's storage, opened cold, takes a while: E2E_HD.)
        await until(() => we.map || we.error, 'the page shows the map', 180000)
        check(!!we.map, `the map shown${we.error ? `, not: ${we.error}` : ''}`)
        await until(() => we.drawn || we.error, 'the terrain drawn', 240000)
        check(!!we.drawn && !we.error, `the terrain drawn${we.error ? `, not: ${we.error}` : ''}`)
    }
    const close = async () => {
        await vscode.commands.executeCommand('wc3.editor.close', {root})
        check(!api.worldEditors.has(root), 'closed: no editor of the project')
        await vscode.commands.executeCommand('workbench.action.closeAllEditors')
    }
    const STEPS = [
        {name: 'fdf', run: async () => {
            log('a frame definition file (.fdf): its language, its colors from the server by a statement\'s shape, a missing comma an error at once (unsaved), gone when it\'s typed')
            const file = path.join(root, 'UI', 'FrameDef', 'Test.fdf')
            fs.mkdirSync(path.dirname(file), {recursive: true})
            fs.writeFileSync(file, [
                '// A test frame',
                'IncludeFile "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",',
                'Frame "GLUETEXTBUTTON" "TestButton" INHERITS WITHCHILDREN "EscMenuButtonTemplate" {',
                '    Width 0.228,',
                '    ButtonPushedTextOffset 0.002f -0.002f,',
                '    SetPoint TOPLEFT, "TestParent", TOPLEFT, 0.0, -0.03,',
                '    Texture {',
                '        File "Test",',
                '    }',
                '}',
                '',
            ].join('\n'))
            const uri = vscode.Uri.file(file)
            const doc = await vscode.workspace.openTextDocument(uri)
            const editor = await vscode.window.showTextDocument(doc)
            check(doc.languageId === 'fdf', `the language: ${doc.languageId}`)
            const legend = await until(() => vscode.commands.executeCommand('vscode.provideDocumentSemanticTokensLegend', uri), 'the legend')
            const tokens = await until(async () => {
                const t = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri)
                return t && t.data.length ? t : null
            }, 'the colors')
            // Decoded: line (1-based), column (1-based), text, type.
            const got = []
            for (let i = 0, line = 0, col = 0; i < tokens.data.length; i += 5) {
                const [dl, dc, len, ty] = tokens.data.slice(i, i + 4)
                line += dl
                col = dl ? dc : col + dc
                got.push(`${line + 1}:${doc.lineAt(line).text.slice(col, col + len)}=${legend.tokenTypes[ty]}`)
            }
            const want = ['1:// A test frame=comment', '2:IncludeFile=property', '3:Frame=keyword', '3:"GLUETEXTBUTTON"=string', '3:INHERITS=enumMember', '3:{=operator',
                '5:0.002f=number', '5:-0.002f=number', '6:SetPoint=property', '6:TOPLEFT=enumMember', '6:-0.03=number', '7:Texture=keyword', '8:File=property', '10:}=operator']
            const missing = want.filter(w => !got.includes(w))
            check(missing.length === 0, `colored: ${want.length} checked${missing.length ? `, missing ${missing.join(' ')} of ${got.join(' ')}` : ''}`)
            await until(() => vscode.languages.getDiagnostics(uri).length === 0 || null, 'no problems', 5000).catch(() => {})
            check(vscode.languages.getDiagnostics(uri).length === 0, `no problems: ${vscode.languages.getDiagnostics(uri).map(d => d.message).join('; ')}`)
            // Width's comma out: an error on its line, unsaved.
            const comma = new vscode.Range(3, 15, 3, 16)
            check(doc.getText(comma) === ',', `the comma after Width: ${doc.getText(comma)}`)
            await editor.edit(e => e.delete(comma))
            const problems = await until(() => vscode.languages.getDiagnostics(uri).filter(d => d.severity === vscode.DiagnosticSeverity.Error).length ? vscode.languages.getDiagnostics(uri) : null, 'the missing comma reported')
            check(problems.length === 1 && problems[0].range.start.line === 3 && problems[0].message.includes("expected ','"), `one error, on Width's line: ${problems.map(d => `${d.range.start.line + 1}: ${d.message}`).join('; ')}`)
            await shot('fdf-missing-comma')
            await editor.edit(e => e.insert(new vscode.Position(3, 15), ','))
            await until(() => vscode.languages.getDiagnostics(uri).length === 0, 'the error gone')
            check(true, 'the comma typed back: no problems')
            await shot('fdf')
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
        }},
        {name: 'fdf-links', run: async () => {
            log('a frame file\'s names: INHERITS to the game\'s template (inside the game\'s archive), its references, hover, the outline a tree, IncludeFile a link, completion in a string, a frame inherited that\'s declared nowhere an error; code\'s strings with a frame\'s name linked to it (JASS, Lua), completion in them')
            const file = path.join(root, 'UI', 'FrameDef', 'Links.fdf')
            fs.mkdirSync(path.dirname(file), {recursive: true})
            fs.writeFileSync(file, [
                'IncludeFile "UI\\FrameDef\\UI\\EscMenuTemplates.fdf",',
                'Frame "GLUETEXTBUTTON" "LinksButton" INHERITS WITHCHILDREN "EscMenuButtonTemplate" {',
                '    ControlBackdrop "LinksBack",',
                '    Frame "BACKDROP" "LinksBack" INHERITS "NoSuchTemplate" {',
                '    }',
                '    Frame "TEXT" "LinksText" {',
                '        Text "ACCEPT",',
                '        SetPoint CENTER, "LinksButton", CENTER, 0, 0,',
                '    }',
                '}',
                '',
            ].join('\n'))
            const uri = vscode.Uri.file(file)
            const doc = await vscode.workspace.openTextDocument(uri)
            await vscode.window.showTextDocument(doc)
            const at = (line, text) => new vscode.Position(line, doc.lineAt(line).text.indexOf(text) + 1)
            const where = l => `${path.basename(l.uri.fsPath)}:${l.range.start.line + 1} (${l.uri.scheme})`
            const defs = await until(async () => {
                const d = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', uri, at(1, 'EscMenuButtonTemplate'))
                return d && d.length ? d : null
            }, 'the template\'s definition', 60000)
            check(defs.length === 1 && defs[0].uri.fsPath.replace(/\\/g, '/').endsWith('UI/FrameDef/UI/EscMenuTemplates.fdf') && /\.mpq\//i.test(defs[0].uri.fsPath), `INHERITS "EscMenuButtonTemplate" → ${defs.map(d => d.uri.fsPath + ':' + (d.range.start.line + 1)).join(', ')}`)
            const target = await vscode.workspace.openTextDocument(defs[0].uri)
            check(target.lineAt(defs[0].range.start.line).text.includes('"EscMenuButtonTemplate"'), `on its line: ${target.lineAt(defs[0].range.start.line).text.trim()}`)
            const key = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', uri, at(6, 'ACCEPT'))
            check(key.length === 1 && path.basename(key[0].uri.fsPath) === 'GlobalStrings.fdf', `Text "ACCEPT" → ${key.map(where).join(', ')}`)
            const refs = await vscode.commands.executeCommand('vscode.executeReferenceProvider', uri, at(3, 'LinksBack'))
            check(refs.length === 2 && refs.every(r => r.uri.fsPath === file), `LinksBack: ${refs.map(r => r.range.start.line + 1).join(', ')}`)
            const hovers = await vscode.commands.executeCommand('vscode.executeHoverProvider', uri, at(7, 'LinksButton'))
            const hover = hovers.flatMap(h => h.contents.map(c => c.value ?? String(c))).join('\n')
            check(hover.includes('Frame "GLUETEXTBUTTON" "LinksButton"'), `hover on "LinksButton": ${hover.split('\n')[1]}`)
            const outline = await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', uri)
            const tree = s => s.children?.length ? `${s.name}(${s.children.map(tree).join(' ')})` : s.name
            check(outline.map(tree).join(' ') === 'LinksButton(LinksBack LinksText)', `the outline: ${outline.map(tree).join(' ')}`)
            const links = await vscode.commands.executeCommand('vscode.executeLinkProvider', uri)
            check(links.length === 1 && links[0].target && links[0].target.path.endsWith('UI/FrameDef/UI/EscMenuTemplates.fdf') && links[0].range.start.line === 0, `IncludeFile → ${links.map(l => l.target?.toString()).join(', ')}`)
            const list = await vscode.commands.executeCommand('vscode.executeCompletionItemProvider', uri, new vscode.Position(1, doc.lineAt(1).text.indexOf('EscMenuButtonTemplate') + 3), '"')
            const labels = list.items.map(i => typeof i.label === 'string' ? i.label : i.label.label)
            check(['EscMenuButtonTemplate', 'LinksBack', 'ACCEPT'].every(n => labels.includes(n)), `completion in the string: ${labels.length} names, ${['EscMenuButtonTemplate', 'LinksBack', 'ACCEPT'].filter(n => labels.includes(n)).join(', ')} among them`)
            const problems = await until(() => vscode.languages.getDiagnostics(uri).length ? vscode.languages.getDiagnostics(uri) : null, 'the unknown template reported')
            check(problems.length === 1 && problems[0].range.start.line === 3 && problems[0].message.includes("'NoSuchTemplate'") && doc.getText(problems[0].range) === 'NoSuchTemplate', `one problem: ${problems.map(d => `${d.range.start.line + 1}: ${d.message} [${doc.getText(d.range)}]`).join('; ')}`)
            // Code: any string whose text is a frame's name is a reference
            // to it (soft links, by the text alone), in every language.
            const jass = path.join(root, 'Frames.j')
            fs.writeFileSync(jass, [
                'function MakeLinks takes nothing returns nothing',
                '    local framehandle b = BlzCreateFrame("LinksButton", BlzGetOriginFrame(ORIGIN_FRAME_GAME_UI, 0), 0, 0)',
                '    local framehandle e = BlzCreateFrame("EscMenuButtonTemplate", b, 0, 0)',
                '    call BlzFrameSetText(b, "LinksButton")',
                '    call BlzFrameSetText(b, "Not a frame")',
                'endfunction',
                '',
            ].join('\n'))
            const lua = path.join(root, 'frames.lua')
            fs.writeFileSync(lua, 'local b = BlzCreateFrame(\'LinksBack\', nil, 0, 0)\n')
            const juri = vscode.Uri.file(jass)
            const jdoc = await vscode.workspace.openTextDocument(juri)
            const jeditor = await vscode.window.showTextDocument(jdoc)
            const jat = (line, text) => new vscode.Position(line, jdoc.lineAt(line).text.indexOf(text) + 2)
            const mine = await until(async () => {
                const d = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', juri, jat(1, 'LinksButton'))
                return d && d.length ? d : null
            }, 'BlzCreateFrame("LinksButton") to its frame', 60000)
            check(mine.length === 1 && mine[0].uri.fsPath === file && mine[0].range.start.line === 1, `BlzCreateFrame("LinksButton") → ${mine.map(where).join(', ')}`)
            const game = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', juri, jat(2, 'EscMenuButtonTemplate'))
            check(game.length === 1 && path.basename(game[0].uri.fsPath) === 'EscMenuTemplates.fdf', `BlzCreateFrame("EscMenuButtonTemplate") → ${game.map(where).join(', ')}`)
            const text = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', juri, jat(3, 'LinksButton'))
            check(text.length === 1 && text[0].uri.fsPath === file, `any string with a frame's name, BlzFrameSetText's too → ${text.map(where).join(', ')}`)
            const none = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', juri, jat(4, 'Not a frame'))
            check(none.length === 0, `"Not a frame" names nothing: ${none.length}`)
            const luri = vscode.Uri.file(lua)
            await vscode.workspace.openTextDocument(luri)
            const fromLua = await until(async () => {
                const d = await vscode.commands.executeCommand('vscode.executeDefinitionProvider', luri, new vscode.Position(0, 27))
                return d && d.length ? d : null
            }, 'Lua\'s \'LinksBack\' to its frame')
            check(fromLua[0].uri.fsPath === file && fromLua[0].range.start.line === 3, `Lua 'LinksBack' → ${fromLua.map(where).join(', ')}`)
            const uses = await vscode.commands.executeCommand('vscode.executeReferenceProvider', uri, at(1, 'LinksButton'))
            check(uses.filter(r => r.uri.fsPath === jass).length === 2, `"LinksButton" used in Frames.j: ${uses.map(where).join(', ')}`)
            // Typing a new one: '"' closes itself; Ctrl+Space in it offers the frames.
            await jeditor.edit(e => e.insert(new vscode.Position(5, 0), '    set e = BlzCreateFrame(, b, 0, 0)\n'))
            jeditor.selection = new vscode.Selection(5, 27, 5, 27)
            await vscode.commands.executeCommand('type', {text: '"'})
            check(jdoc.lineAt(5).text.includes('BlzCreateFrame("", b'), `'"' closed: ${jdoc.lineAt(5).text.trim()}`)
            const offered = await until(async () => {
                const l = await vscode.commands.executeCommand('vscode.executeCompletionItemProvider', juri, new vscode.Position(5, 28))
                const names = l ? l.items.filter(i => i.kind === vscode.CompletionItemKind.Class).map(i => typeof i.label === 'string' ? i.label : i.label.label) : []
                return names.length ? names : null
            }, 'frames offered in the string')
            check(['LinksButton', 'EscMenuButtonTemplate'].every(n => offered.includes(n)), `offered in BlzCreateFrame(""): ${offered.length} frames, LinksButton and EscMenuButtonTemplate among them`)
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
            await vscode.commands.executeCommand('outline.focus')
            await vscode.window.showTextDocument(doc)
            await vscode.commands.executeCommand('outline.expand')
            await sleep(1500)
            await shot('fdf-links')
            await vscode.commands.executeCommand('workbench.action.closeAllEditors')
        }},
        {name: 'toc', run: async () => {
            log('a list of frame files (.toc): its language, its lines colored and links to the files (the game\'s inside its archive, the project\'s), a file that\'s nowhere and a last line without a break warned of, BlzLoadTOCFile\'s path a link')
            const dir = path.join(root, 'war3mapImported')
            fs.mkdirSync(dir, {recursive: true})
            fs.writeFileSync(path.join(dir, 'Menu.fdf'), 'Frame "BACKDROP" "MenuBack" {\n}\n')
            // (The test project has no sources: an open file is in the model.)
            await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(dir, 'Menu.fdf')))
            const file = path.join(dir, 'Menu.toc')
            fs.writeFileSync(file, 'UI\\FrameDef\\UI\\EscMenuTemplates.fdf\r\nwar3mapImported\\Menu.fdf\r\nwar3mapImported\\Nowhere.fdf')
            const uri = vscode.Uri.file(file)
            const doc = await vscode.workspace.openTextDocument(uri)
            await vscode.window.showTextDocument(doc)
            check(doc.languageId === 'wc3-toc', `the language: ${doc.languageId}`)
            const tokens = await until(async () => {
                const t = await vscode.commands.executeCommand('vscode.provideDocumentSemanticTokens', uri)
                return t && t.data.length ? t : null
            }, 'the colors')
            check(tokens.data.length === 15, `three lines colored: ${tokens.data.length / 5}`)
            const problems = await until(() => vscode.languages.getDiagnostics(uri).length >= 2 ? vscode.languages.getDiagnostics(uri) : null, 'the problems', 60000)
            const said = problems.map(d => `${d.range.start.line + 1}: ${d.code?.value ?? d.code ?? ''} ${d.message}`)
            check(problems.length === 2 && problems.every(d => d.range.start.line === 2) && said.some(x => x.includes('Nowhere.fdf')) && said.some(x => x.includes('line break')), `the last line: ${said.join('; ')}`)
            const links = await vscode.commands.executeCommand('vscode.executeLinkProvider', uri)
            const to = links.map(l => `${l.range.start.line + 1} → ${l.target.scheme}:${path.basename(l.target.path)}`)
            check(links.length === 2 && to[0] === '1 → wc3map:EscMenuTemplates.fdf' && to[1] === '2 → file:Menu.fdf', `links: ${to.join(', ')}`)
            // The game's own list is read as one.
            const target = await vscode.workspace.openTextDocument(links[0].target)
            check(target.languageId === 'fdf', `the game's EscMenuTemplates.fdf opens as FDF: ${target.languageId}`)
            await shot('toc')
            // A line break at the end: the warning goes.
            const editor = await vscode.window.showTextDocument(doc)
            await editor.edit(e => e.insert(doc.lineAt(2).range.end, '\r\n'))
            await until(() => vscode.languages.getDiagnostics(uri).length === 1 || null, 'only the missing file left')
            check(vscode.languages.getDiagnostics(uri)[0].message.includes('Nowhere.fdf'), 'the line break typed: only the missing file left')
            // In code, BlzLoadTOCFile's path opens the list.
            const jass = path.join(root, 'LoadToc.j')
            fs.writeFileSync(jass, 'function LoadMenu takes nothing returns nothing\n    call BlzLoadTOCFile("war3mapImported\\\\Menu.toc")\nendfunction\n')
            const juri = vscode.Uri.file(jass)
            await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(juri))
            const jlinks = await until(async () => {
                const l = await vscode.commands.executeCommand('vscode.executeLinkProvider', juri)
                return l && l.length ? l : null
            }, 'the path in BlzLoadTOCFile a link')
            check(jlinks.length === 1 && jlinks[0].target.fsPath === file, `BlzLoadTOCFile("war3mapImported\\\\Menu.toc") → ${jlinks.map(l => path.basename(l.target.fsPath)).join(', ')}`)
            await vscode.commands.executeCommand('workbench.action.revertAndCloseActiveEditor')
            await vscode.commands.executeCommand('workbench.action.closeAllEditors')
        }},
    ]
    // What's run: `only`'s names (all without it), with what they need.
    const names = new Set(only ?? STEPS.map(st => st.name))
    for (const n of names) if (!STEPS.some(st => st.name === n)) throw new Error(`no step ${n}: ${STEPS.map(st => st.name).join(', ')}`)
    for (const st of [...STEPS].reverse()) if (names.has(st.name)) for (const n of st.needs ?? []) names.add(n)
    const chosen = STEPS.filter(st => names.has(st.name))
    log(`steps: ${chosen.map(st => st.name).join(', ')}`)
    let opened = false
    for (const st of chosen) {
        if (st.editor && !opened) {
            await open()
            opened = true
        } else if (!st.editor && opened) {
            await close()
            opened = false
        }
        await st.run()
    }
    if (opened) await close()
}

module.exports.run = async () => {
    log(`e2e ${new Date().toISOString()}`)
    try {
        const which = process.env.E2E_STEPS || 'all'
        await steps(which === 'all' ? null : which === 'current' ? CURRENT : which.split(','))
        log('all passed')
    } catch (e) {
        log(`FAILED: ${e.stack || e.message}`)
        await shot('failed')
        throw e
    }
}
