// The txt2 preview: the server's HTML for the text (`QUERY.TXT2_HTML`),
// each block marked with the line it starts at (`data-line`). The page
// and the editor follow each other's scrolling; a double click puts the
// editor's cursor on the line; links open outside.
const vscode = acquireVsCodeApi()
const body = document.getElementById('content')
let blocks = []
// While the page scrolls to follow the editor, its own scrolling isn't
// sent back.
let following = 0

function index() {
    blocks = [...body.querySelectorAll('[data-line]')].map(el => ({el, line: Number(el.dataset.line)}))
}

/** The block a line is in: the last one starting at or before it. */
function blockAt(line) {
    let found = null
    for (const b of blocks) {
        if (b.line > line) break
        if (!found || b.line >= found.line) found = b
    }
    return found
}

function scrollToLine(line) {
    const b = blockAt(line)
    following = Date.now()
    if (!b) return window.scrollTo(0, 0)
    window.scrollTo(0, b.el.getBoundingClientRect().top + window.scrollY - 8)
}

window.addEventListener('message', ({data}) => {
    if (data.type === 'html') {
        const y = window.scrollY
        body.innerHTML = data.html
        index()
        window.scrollTo(0, y)
        if (data.line != null) scrollToLine(data.line)
    } else if (data.type === 'scroll') {
        scrollToLine(data.line)
    }
})

let timer = 0
window.addEventListener('scroll', () => {
    if (Date.now() - following < 300) return
    clearTimeout(timer)
    timer = setTimeout(() => {
        const top = blocks.find(b => b.el.getBoundingClientRect().bottom > 0)
        if (top) vscode.postMessage({type: 'reveal', line: top.line})
    }, 60)
})

document.addEventListener('dblclick', e => {
    const el = e.target.closest('[data-line]')
    if (el) vscode.postMessage({type: 'select', line: Number(el.dataset.line)})
})

document.addEventListener('click', e => {
    const a = e.target.closest('a[href]')
    if (!a) return
    e.preventDefault()
    vscode.postMessage({type: 'open', href: a.getAttribute('href')})
})

vscode.postMessage({type: 'ready'})
