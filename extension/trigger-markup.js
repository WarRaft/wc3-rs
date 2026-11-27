// @ts-check
'use strict'

/**
 * The trigger editor's markup (`webview/triggers.js` mounts on
 * it), for every page it's in: its own tab (`extension.js`) and the World
 * Editor's Triggers panel (`world-editor.js`). Its root, a `trg-root`
 * element (with `wtg`: a map's tree beside the trigger), is the page's.
 *
 * @param {boolean} wtg  a map's triggers, their tree beside
 * @param {string} langs  the Show JASS / Lua / AngelScript buttons
 */
function triggerMarkup(wtg, langs) {
    return `
${wtg ? `<div class="map-side">
<div class="map-tools">
  <button id="map-new-category" title="New category"><i class="fa-solid fa-folder-plus"></i></button>
  <button id="map-new-trigger" title="New trigger, in the category selected"><i class="fa-solid fa-file-circle-plus"></i></button>
  <button id="map-new-variable" title="New variable"><i class="fa-solid fa-cube"></i></button>
  <button id="map-rename" title="Rename (F2)"><i class="fa-solid fa-pen"></i></button>
  <button id="map-delete" title="Delete (Delete)"><i class="fa-solid fa-trash"></i></button>
</div>
<nav id="map-tree" class="map-tree" tabindex="0" role="tree"></nav>
</div>
<div id="map-splitter" class="map-splitter" title="Drag to size the tree"></div>
<div class="main">` : ''}
<div class="bar">
  <span id="title-icon" class="title-icon"><i class="fa-solid fa-bolt"></i></span>
  <input id="name" class="name" spellcheck="false" title="The trigger's name">
  <label class="check" title="Is the trigger on at all?"><input id="enabled" type="checkbox">Enabled</label>
  <label class="check" title="Does the trigger start on? Off, something must turn it on"><input id="initially-on" type="checkbox">Initially On</label>
  <div class="tools">
    <button id="new" title="New event, condition or action (Ctrl+Shift+N)"><i class="fa-solid fa-plus"></i></button>
    <button id="new-variable" title="New variable"><i class="fa-solid fa-cube"></i></button>
    <button id="toggle" title="Enable or disable (Ctrl+E)"><i class="fa-solid fa-power-off"></i></button>
    <button id="up" title="Move up (Alt+Up)"><i class="fa-solid fa-arrow-up"></i></button>
    <button id="down" title="Move down (Alt+Down)"><i class="fa-solid fa-arrow-down"></i></button>
    <button id="copy" title="Copy (Ctrl+C)"><i class="fa-solid fa-copy"></i></button>
    <button id="cut" title="Cut (Ctrl+X)"><i class="fa-solid fa-scissors"></i></button>
    <button id="paste" title="Paste (Ctrl+V)"><i class="fa-solid fa-paste"></i></button>
    <button id="delete" title="Delete (Delete)"><i class="fa-solid fa-trash"></i></button>
    <button id="convert" title="Convert to Custom Text: the trigger's events, conditions and actions become its JASS"><i class="fa-solid fa-code"></i></button>
  </div>
  <div class="spacer"></div>
  <span id="problems" class="problems" hidden></span>
  ${wtg ? `<button id="extract" class="text-button" title="Every trigger as a file of its own (.trg), in a folder you pick">Extract Triggers…</button>` : ''}
  <div class="langs">${langs}</div>
  <button id="open-text" class="text-button" title="Open the text of this file">Open as Text</button>
</div>
<textarea id="description" class="description" rows="2" spellcheck="false" placeholder="Comment"></textarea>
<div id="notice" class="notice" hidden></div>
<div id="tree" class="tree" tabindex="0" role="tree"></div>
<div id="loading" class="message">Reading the trigger…</div>
${wtg ? '</div>' : ''}
<div id="dialog" class="overlay" hidden><form id="dialog-box" class="dialog"></form></div>`
}

/**
 * The stylesheets and the icons' font it needs, for a page's head.
 *
 * @param {(name: string) => string} asset  a file of `webview/`, as the page reaches it
 */
function triggerHead(asset) {
    const fa = name => asset(`fontawesome/${name}`)
    return `<link rel="stylesheet" href="${fa('fontawesome.min.css')}">
<link rel="stylesheet" href="${asset('triggers.css')}">
<style>
@font-face { font-family: "Font Awesome 6 Free"; font-style: normal; font-weight: 900; font-display: block; src: url("${fa('fa-solid-900.woff2')}") format("woff2"); }
.fa-solid { font-family: "Font Awesome 6 Free"; font-weight: 900; }
</style>`
}

module.exports = {triggerMarkup, triggerHead}
