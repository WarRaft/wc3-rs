/**
 * The colors of code's tokens in the current color theme, for pages
 * (webviews) that draw code: VS Code gives them its UI's colors as CSS
 * variables, not the editor's token colors. So the theme's own file is
 * read (its `include`s too) as VS Code reads it for semantic
 * highlighting: a token's `semanticTokenColors` rule (`variable.readonly`,
 * `*.declaration`, `function.defaultLibrary:jass`), else the TextMate
 * scopes the standard type maps to (`variable` →
 * `variable.other.readwrite`, ...), the most specific `tokenColors` rule
 * of them; the user's `editor.semanticTokenColorCustomizations` and
 * `editor.tokenColorCustomizations` over the theme's. What comes out is
 * CSS for the `tok` classes (`.tok-variable.tok-readonly`), each token a
 * `tok` span with a `tok-<type>` and a `tok-<modifier>` class each.
 */
const fs = require('fs')
const path = require('path')

/** VS Code's TextMate scopes for the standard semantic token types (its `tokenClassificationRegistry`), first found wins. */
const SCOPES = {
    namespace: ['entity.name.namespace'],
    type: ['entity.name.type', 'support.type'],
    'type.defaultLibrary': ['support.type'],
    struct: ['entity.name.type.struct', 'entity.name.type'],
    class: ['entity.name.type.class', 'entity.name.type'],
    interface: ['entity.name.type.interface', 'entity.name.type'],
    enum: ['entity.name.type.enum', 'entity.name.type'],
    typeParameter: ['entity.name.type.parameter', 'entity.name.type'],
    function: ['entity.name.function', 'support.function'],
    'function.defaultLibrary': ['support.function'],
    method: ['entity.name.function.member', 'support.function'],
    macro: ['entity.name.function.preprocessor', 'meta.preprocessor'],
    variable: ['variable.other.readwrite', 'entity.name.variable', 'variable'],
    'variable.readonly': ['variable.other.constant'],
    'variable.readonly.defaultLibrary': ['support.constant'],
    parameter: ['variable.parameter'],
    property: ['variable.other.property'],
    'property.readonly': ['variable.other.constant.property'],
    enumMember: ['variable.other.enummember'],
    event: ['variable.other.event'],
    label: ['entity.name.label'],
    keyword: ['keyword'],
    comment: ['comment'],
    string: ['string'],
    number: ['constant.numeric'],
    regexp: ['constant.regexp'],
    operator: ['keyword.operator'],
}

/** JSON with comments and trailing commas, as themes are written. */
function parseJsonc(text) {
    let out = ''
    for (let i = 0; i < text.length; i++) {
        const c = text[i]
        if (c === '"') {
            const start = i
            for (i++; i < text.length && text[i] !== '"'; i++) if (text[i] === '\\') i++
            out += text.slice(start, i + 1)
        } else if (c === '/' && text[i + 1] === '/') {
            while (i < text.length && text[i] !== '\n') i++
            out += '\n'
        } else if (c === '/' && text[i + 1] === '*') {
            i = text.indexOf('*/', i + 2)
            if (i < 0) break
            i++
        } else {
            out += c
        }
    }
    return JSON.parse(out.replace(/,(\s*[}\]])/g, '$1'))
}

/**
 * A theme's file read with what it includes: its `tokenColors` (rules,
 * the included file's first) and `semanticTokenColors` (the file's over
 * the included's). A file that can't be read gives nothing.
 */
function readTheme(file, seen = new Set()) {
    const theme = {tokenColors: [], semanticTokenColors: {}}
    if (seen.has(file)) return theme
    seen.add(file)
    let json
    try {
        json = parseJsonc(fs.readFileSync(file, 'utf8'))
    } catch {
        return theme
    }
    if (typeof json.include === 'string') {
        const inner = readTheme(path.join(path.dirname(file), json.include), seen)
        theme.tokenColors.push(...inner.tokenColors)
        Object.assign(theme.semanticTokenColors, inner.semanticTokenColors)
    }
    if (Array.isArray(json.tokenColors)) theme.tokenColors.push(...json.tokenColors)
    // (A `.tmTheme` file's rules aren't read: their colors stay the page's.)
    if (json.semanticTokenColors && typeof json.semanticTokenColors === 'object') Object.assign(theme.semanticTokenColors, json.semanticTokenColors)
    return theme
}

/**
 * The file of the theme named `name` (the `workbench.colorTheme`
 * setting: a theme's `id`, else its `label`) among `extensions`
 * (`vscode.extensions.all`), or `null`.
 */
function themeFile(name, extensions) {
    for (const ext of extensions) {
        const themes = ext.packageJSON?.contributes?.themes
        if (!Array.isArray(themes)) continue
        // (Older settings name a built-in theme `Default Dark Modern`.)
        const t = themes.find(t => [t.id, t.label].some(n => n && (n === name || `Default ${n}` === name)))
        if (t && t.path) return path.join(ext.extensionPath, t.path)
    }
    return null
}

/** Does the TextMate rule's scope selector `selector` (`variable`, `a.b c` — its last part) match `scope`? How specific: its length, -1 if not. */
function matchScope(selector, scope) {
    const last = selector.trim().split(/\s+/).pop() || ''
    return scope === last || scope.startsWith(`${last}.`) ? last.length : -1
}

/** A rule's style: `{foreground, fontStyle}` from a color string or an object. */
function styleOf(v) {
    if (typeof v === 'string') return {foreground: v}
    if (!v || typeof v !== 'object') return null
    const fontStyle = typeof v.fontStyle === 'string' ? v.fontStyle : [v.bold && 'bold', v.italic && 'italic', v.underline && 'underline', v.strikethrough && 'strikethrough'].filter(Boolean).join(' ') || undefined
    return {foreground: v.foreground, fontStyle}
}

/** CSS declarations of a style. */
function declarations(style) {
    const out = []
    if (style.foreground) out.push(`color: ${style.foreground}`)
    if (style.fontStyle !== undefined) {
        const words = style.fontStyle.split(/\s+/)
        out.push(`font-style: ${words.includes('italic') ? 'italic' : 'normal'}`)
        out.push(`font-weight: ${words.includes('bold') ? 'bold' : 'normal'}`)
        const lines = ['underline', 'strikethrough'].filter(w => words.includes(w)).map(w => (w === 'strikethrough' ? 'line-through' : w))
        out.push(`text-decoration: ${lines.length ? lines.join(' ') : 'none'}`)
    }
    return out.join('; ')
}

/**
 * The CSS of the token classes in `theme` (`readTheme`'s) with the user's
 * customizations (`semantic`: `editor.semanticTokenColorCustomizations`,
 * `textmate`: `editor.tokenColorCustomizations`, as their settings are),
 * for `language` (`jass`: a rule `x:jass` counts, `x:lua` doesn't).
 */
function css(theme, semantic = {}, textmate = {}, language = 'jass') {
    // The user's TextMate rules after the theme's (later wins in a tie).
    const rules = [...theme.tokenColors, ...(Array.isArray(textmate?.textMateRules) ? textmate.textMateRules : [])]
    const lines = []
    for (const [key, scopes] of Object.entries(SCOPES)) {
        // The first scope a rule matches: its most specific rule (a tie:
        // the later), its color and font style each from the most
        // specific rule that sets it.
        let best = null
        for (const scope of scopes) {
            let color = {depth: -1, value: undefined}
            let font = {depth: -1, value: undefined}
            for (const r of rules) {
                const style = r && styleOf(r.settings)
                if (!style) continue
                const selectors = Array.isArray(r.scope) ? r.scope : typeof r.scope === 'string' ? r.scope.split(',') : []
                const d = Math.max(-1, ...selectors.map(s => matchScope(s, scope)))
                if (d < 0) continue
                if (style.foreground && d >= color.depth) color = {depth: d, value: style.foreground}
                if (style.fontStyle !== undefined && d >= font.depth) font = {depth: d, value: style.fontStyle}
            }
            if (color.value || font.value !== undefined) {
                best = {foreground: color.value, fontStyle: font.value}
                break
            }
        }
        if (best) lines.push(`${selector(key)} { ${declarations(best)} }`)
    }
    // Semantic rules: the theme's, then the user's (`rules`, or a theme's own under `[Theme Name]`, not read).
    const semanticRules = {...theme.semanticTokenColors, ...(semantic && typeof semantic.rules === 'object' ? semantic.rules : {})}
    const ordered = Object.entries(semanticRules)
        .map(([key, v]) => {
            const [sel, lang] = key.split(':')
            return {sel, lang, style: styleOf(v)}
        })
        .filter(r => r.style && (!r.lang || r.lang === language))
        // Fewer modifiers first: the more specific after (and with more classes).
        .sort((a, b) => a.sel.split('.').length - b.sel.split('.').length)
    for (const r of ordered) lines.push(`${selector(r.sel)} { ${declarations(r.style)} }`)
    return lines.join('\n')
}

/** `variable.readonly` → `.tok.tok-variable.tok-readonly`; `*.declaration` → `.tok.tok-declaration`. */
function selector(key) {
    const [type, ...mods] = key.split('.')
    return ['.tok', type === '*' ? '' : `.tok-${type}`, ...mods.map(m => `.tok-${m}`)].join('')
}

module.exports = {css, readTheme, themeFile, parseJsonc, selector}
