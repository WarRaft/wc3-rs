import os, sys
# The extension's file icons, one style: a 32x32 grid, flat shapes, one
# color per icon (Material palette) with white or lighter parts for
# detail. Code is a rounded badge with the language's mark; text data a
# sheet with a folded corner; everything else its own symbol. Run
# `python3 make.py .` in this folder to write them all.
OUT = sys.argv[1]
W = '#fff'

def svg(body):
    return '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32">\n' + body + '\n</svg>\n'

def badge(c, glyph):
    return f'  <rect x="3" y="3" width="26" height="26" rx="6" fill="{c}"/>\n' + glyph

def doc(c, glyph):
    return (f'  <path fill="{c}" d="M8 2h12l7 7v19a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z"/>\n'
            f'  <path fill="{W}" opacity=".45" d="M20 2v5a2 2 0 0 0 2 2h5z"/>\n' + glyph)

def image(c):
    return (f'  <rect x="3" y="5" width="26" height="22" rx="3" fill="{c}"/>\n'
            f'  <path fill="{W}" opacity=".9" d="M6 24l7-9 5 6 3-3 5 6z"/>\n'
            f'  <circle cx="22" cy="11" r="2.5" fill="{W}" opacity=".9"/>')

J = '<path fill="#fff" d="M17 8h5v11.5a5.5 5.5 0 0 1-11 0V18h4v1.5a1.5 1.5 0 0 0 3 0z"/>'
icons = {
    # Code: a colored badge with the language's mark.
    'jass': badge('#FF9800', '  ' + J),
    'vjass': badge('#F4511E', '  ' + J + '\n  <path fill="#fff" d="M8 8h2v2.5h2.5v2H10V15H8v-2.5H5.5v-2H8z"/>'),
    'zinc': badge('#00897B', '  <path fill="#fff" d="M9 8h14v3.5L14.5 20.5H23V24H9v-3.5l8.5-9H9z"/>'),
    'lua': badge('#3949AB', '  <circle cx="15" cy="17" r="8" fill="#fff"/>\n  <circle cx="18.5" cy="13.5" r="2.6" fill="#3949AB"/>\n  <circle cx="24" cy="8" r="2.4" fill="#fff"/>'),
    'as': badge('#8E24AA', '  <path fill="#fff" fill-rule="evenodd" d="M14 7h4l7 18h-4.2l-1.4-4h-6.8l-1.4 4H7zm2 4.5L13.8 17.5h4.4z"/>'),
    'wct': badge('#FFB300', '  <path fill="none" stroke="#fff" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" d="M13 8c-2.5 0-3.5 1-3.5 3.5v2c0 1.2-.8 2.2-2 2.5 1.2.3 2 1.3 2 2.5v2c0 2.5 1 3.5 3.5 3.5M19 8c2.5 0 3.5 1 3.5 3.5v2c0 1.2.8 2.2 2 2.5-1.2.3-2 1.3-2 2.5v2c0 2.5-1 3.5-3.5 3.5"/>'),
    # Text data: a sheet with a folded corner and what it holds.
    'bni': doc('#78909C', '  <path fill="#fff" d="M10 13h3.5v2H12v8h1.5v2H10zM22 13h-3.5v2H20v8h-1.5v2H22z"/>\n  <rect x="14.5" y="18" width="3" height="2" rx="1" fill="#fff"/>'),
    'slk': doc('#43A047', '  <path fill="none" stroke="#fff" stroke-width="1.8" d="M10 13.5h12v12H10zM10 17.5h12M10 21.5h12M14 13.5v12M18 13.5v12"/>'),
    'wts': doc('#FDD835', '  <path fill="#fff" d="M10 16.5a3 3 0 1 1 5 2.2c-.3 2.2-1.4 3.8-3.4 4.8l-.8-1.3c1.1-.7 1.8-1.6 2-2.8a3 3 0 0 1-2.8-2.9zM17 16.5a3 3 0 1 1 5 2.2c-.3 2.2-1.4 3.8-3.4 4.8l-.8-1.3c1.1-.7 1.8-1.6 2-2.8a3 3 0 0 1-2.8-2.9z"/>'),
    'fdf': doc('#00ACC1', '  <rect x="9.5" y="13" width="13" height="12" rx="1.5" fill="none" stroke="#fff" stroke-width="1.8"/>\n  <rect x="9.5" y="13" width="13" height="3.2" rx="1" fill="#fff"/>\n  <rect x="12.5" y="19.5" width="7" height="2.6" rx="1" fill="#fff"/>'),
    'toc': doc('#0097A7', '  <circle cx="11" cy="15" r="1.4" fill="#fff"/><circle cx="11" cy="19.5" r="1.4" fill="#fff"/><circle cx="11" cy="24" r="1.4" fill="#fff"/>\n  <path stroke="#fff" stroke-width="2" stroke-linecap="round" d="M14.5 15h8M14.5 19.5h8M14.5 24h8"/>'),
    # Object data.
    'w3u': '  <path fill="#E53935" d="M16 3C9 3 5 8 5 15v5l3 2v6h5v-6h6v6h5v-6l3-2v-5C27 8 23 3 16 3z"/>\n  <rect x="8.5" y="13" width="15" height="3" rx="1.5" fill="#fff" opacity=".85"/>\n  <path fill="#fff" opacity=".35" d="M16 3c-1 0-1.5.6-1.5 1.5V13h3V4.5C17.5 3.6 17 3 16 3z"/>',
    'w3t': '  <rect x="12.5" y="2" width="7" height="3.5" rx="1" fill="#FFB300" opacity=".7"/>\n  <path fill="#FFB300" d="M13 6h6v5.5l6.4 9.6A5.5 5.5 0 0 1 20.8 29h-9.6a5.5 5.5 0 0 1-4.6-7.9L13 11.5z"/>\n  <path fill="#fff" opacity=".45" d="M8.7 21.5h14.6l.8 1.3a3 3 0 0 1-2.6 4.2H10.5a3 3 0 0 1-2.6-4.2z"/>\n  <circle cx="14" cy="15.5" r="1.5" fill="#fff" opacity=".8"/>',
    'w3a': '  <path fill="#7E57C2" d="M14 2l3 10 10 4-10 4-3 10-3-10L1 16l10-4z"/>\n  <path fill="#7E57C2" opacity=".6" d="M25 2l1.3 3.7L30 7l-3.7 1.3L25 12l-1.3-3.7L20 7l3.7-1.3z"/>',
    'w3h': '  <circle cx="16" cy="16" r="12.5" fill="none" stroke="#26C6DA" stroke-width="2.6" stroke-dasharray="5 3"/>\n  <path fill="none" stroke="#26C6DA" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" d="M10.5 16l5.5-5.5 5.5 5.5M10.5 22l5.5-5.5 5.5 5.5"/>',
    'w3q': '  <path fill="#66BB6A" d="M16 2l11 11h-6.5v8h-9v-8H5z"/>\n  <rect x="11.5" y="23" width="9" height="2.6" rx="1.3" fill="#66BB6A" opacity=".75"/>\n  <rect x="11.5" y="27" width="9" height="2.6" rx="1.3" fill="#66BB6A" opacity=".5"/>',
    'w3b': '  <rect x="4" y="4" width="24" height="24" rx="2.5" fill="#8D6E63"/>\n  <rect x="8" y="8" width="16" height="16" fill="none" stroke="#fff" stroke-width="1.6" opacity=".5"/>\n  <path stroke="#fff" stroke-width="2.4" opacity=".6" d="M8.5 8.5l15 15M23.5 8.5l-15 15"/>',
    'w3d': '  <path fill="#7CB342" d="M16 2l7.5 9.5H20l6 8H6l6-8H8.5z"/>\n  <rect x="14" y="19" width="4" height="9" rx="1" fill="#795548"/>',
    # Map parts.
    'w3e': '  <path fill="#8BC34A" d="M1 27l10-15 5.5 7.5 4-5L31 27z"/>\n  <path fill="#fff" opacity=".7" d="M11 12l3.2 4.4-1.7 1-1.5-1.6-1.6 1.7-1.6-.8z"/>\n  <circle cx="24" cy="8" r="3" fill="#FFB300"/>',
    'w3i': '  <circle cx="16" cy="16" r="13.5" fill="#1E88E5"/>\n  <circle cx="16" cy="9.5" r="2.2" fill="#fff"/>\n  <rect x="13.8" y="13.5" width="4.4" height="11" rx="1.5" fill="#fff"/>',
    'w3r': '  <rect x="5" y="6" width="22" height="20" fill="#FF7043" opacity=".25"/>\n  <rect x="5" y="6" width="22" height="20" fill="none" stroke="#FF7043" stroke-width="2.4" stroke-dasharray="4 3"/>\n  <rect x="2" y="3" width="6" height="6" rx="1" fill="#FF7043"/><rect x="24" y="3" width="6" height="6" rx="1" fill="#FF7043"/><rect x="2" y="23" width="6" height="6" rx="1" fill="#FF7043"/><rect x="24" y="23" width="6" height="6" rx="1" fill="#FF7043"/>',
    'w3c': '  <path fill="#29B6F6" d="M9 4a5 5 0 1 0 0 10 5 5 0 0 0 0-10zm11 2a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM3 15a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h18a2 2 0 0 0 2-2v-2.2l7 4.2V13l-7 4.2V17a2 2 0 0 0-2-2z"/>',
    'w3s': '  <path fill="#AB47BC" d="M2 11h6l9-8v26l-9-8H2z"/>\n  <path fill="none" stroke="#AB47BC" stroke-width="2.5" stroke-linecap="round" d="M21 11.5a6 6 0 0 1 0 9M24.5 7.5a11 11 0 0 1 0 17M28 4a16 16 0 0 1 0 24"/>',
    'wpm': ''.join(f'  <rect x="{3 + 9.33 * (i % 3):.2f}" y="{3 + 9.33 * (i // 3):.2f}" width="7.4" height="7.4" rx="1.5" fill="#90A4AE" opacity="{o}"/>\n' for i, o in enumerate([1, .35, 1, .35, .35, 1, 1, .35, .35])).rstrip(),
    'wtg': '  <path fill="#FFA000" d="M19 1L5 18h9l-3 13 15-18h-9z"/>',
    'shd': '  <path fill="#78909C" d="M13 26h17l-6-9H13z" opacity=".55"/>\n  <rect x="5" y="4" width="8" height="22" rx="2" fill="#B0BEC5"/>\n  <circle cx="26" cy="7" r="3.5" fill="#FFCA28"/>',
    'mmp': '  <path fill="#26A69A" d="M16 2a9.5 9.5 0 0 0-9.5 9.5C6.5 19 16 30 16 30s9.5-11 9.5-18.5A9.5 9.5 0 0 0 16 2z"/>\n  <circle cx="16" cy="11.5" r="3.6" fill="#fff"/>',
    'imp': '  <path fill="none" stroke="#A1887F" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" d="M16 3v14M10 11l6 6 6-6M4 19v6a3 3 0 0 0 3 3h18a3 3 0 0 0 3-3v-6"/>',
    'w3o': '  <path fill="#5E35B1" d="M16 3l13 7-13 7-13-7z"/>\n  <path fill="none" stroke="#5E35B1" stroke-width="2.5" stroke-linejoin="round" opacity=".75" d="M3 16l13 7 13-7"/>\n  <path fill="none" stroke="#5E35B1" stroke-width="2.5" stroke-linejoin="round" opacity=".5" d="M3 22l13 7 13-7"/>',
    'doo': '  <path fill="#7CB342" d="M10 3l6 8h-3l5 7H2l5-7H4z"/>\n  <path fill="#7CB342" opacity=".7" d="M22 7l6 8h-3l5 7H14l5-7h-3z"/>\n  <rect x="8.5" y="18" width="3" height="7" rx="1" fill="#795548"/><rect x="20.5" y="22" width="3" height="7" rx="1" fill="#795548"/>',
    'units': '  <circle cx="16" cy="8" r="5.5" fill="#E53935"/>\n  <path fill="#E53935" d="M5 29c0-6.5 4.9-11 11-11s11 4.5 11 11z"/>',
    'wai': '  <rect x="8" y="8" width="16" height="16" rx="3" fill="#FB8C00"/>\n  <rect x="12" y="12" width="8" height="8" rx="1.5" fill="#fff" opacity=".55"/>\n  <path stroke="#FB8C00" stroke-width="2.4" stroke-linecap="round" d="M12 3v4M16 3v4M20 3v4M12 25v4M16 25v4M20 25v4M3 12h4M3 16h4M3 20h4M25 12h4M25 16h4M25 20h4"/>',
    'preview': image('#29B6F6'),
    # Archives.
    'w3x': '  <path fill="#FBC02D" d="M2 7l9-4 10 4 9-4v22l-9 4-10-4-9 4z"/>\n  <path stroke="#fff" stroke-width="1.6" opacity=".5" d="M11 3v22M21 7v22"/>\n  <path stroke="#E53935" stroke-width="2.4" stroke-linecap="round" d="M14 12l4 4M18 12l-4 4"/>',
    'w3m': '  <path fill="#A1887F" d="M2 7l9-4 10 4 9-4v22l-9 4-10-4-9 4z"/>\n  <path stroke="#fff" stroke-width="1.6" opacity=".5" d="M11 3v22M21 7v22"/>\n  <path stroke="#5D4037" stroke-width="2.4" stroke-linecap="round" d="M14 12l4 4M18 12l-4 4"/>',
    'w3n': '  <path fill="#7E57C2" d="M6 3h16a4 4 0 0 1 4 4v19H10a4 4 0 0 1-4-4z"/>\n  <path fill="#fff" opacity=".7" d="M10 24h16v5H10a2.5 2.5 0 0 1 0-5z"/>\n  <path fill="#fff" opacity=".85" d="M16 8l1.5 3.4 3.7.4-2.8 2.5.8 3.7-3.2-1.9-3.2 1.9.8-3.7-2.8-2.5 3.7-.4z"/>',
    'w3f': '  <path fill="#9575CD" d="M6 3h16a4 4 0 0 1 4 4v19H10a4 4 0 0 1-4-4z"/>\n  <path fill="#fff" opacity=".7" d="M10 24h16v5H10a2.5 2.5 0 0 1 0-5z"/>\n  <circle cx="16" cy="8.5" r="1.8" fill="#fff"/>\n  <rect x="14.3" y="11.5" width="3.4" height="8" rx="1.2" fill="#fff"/>',
    'mpq': '  <path fill="#607D8B" d="M3 9l13-6 13 6v14l-13 6-13-6z"/>\n  <path fill="#fff" opacity=".35" d="M3 9l13 6 13-6-13-6z"/>\n  <path stroke="#fff" stroke-width="1.6" opacity=".5" d="M16 15v14M9.5 6l13 6"/>',
    # Media.
    'blp': image('#EC407A'),
    'tga': image('#26A69A'),
    'mdx': '  <path fill="#26A69A" opacity=".55" d="M16 3l12 6-12 6-12-6z"/>\n  <path fill="#26A69A" d="M4 9l12 6v14L4 23z"/>\n  <path fill="#26A69A" opacity=".8" d="M28 9L16 15v14l12-6z"/>',
    'audio': '  <path fill="#D81B60" d="M11 7l15-4v18.5a4.5 4.5 0 1 1-3-4.2V8.8l-9 2.4v13.3a4.5 4.5 0 1 1-3-4.2z"/>',
    'w3g': '  <circle cx="16" cy="16" r="13.5" fill="#43A047"/>\n  <path fill="#fff" d="M12.5 9.5l10 6.5-10 6.5z"/>',
    'w3z': '  <path fill="#1E88E5" d="M5 3h18l6 6v18a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2z"/>\n  <rect x="9" y="3" width="12" height="8" rx="1" fill="#fff" opacity=".85"/>\n  <rect x="8" y="17" width="16" height="10" rx="1.5" fill="#fff" opacity=".4"/>',
    'w3v': '  <path fill="#FFA000" opacity=".7" d="M4 13c0-5 5-8.5 12-8.5S28 8 28 13z"/>\n  <rect x="4" y="13" width="24" height="15" rx="2" fill="#FFA000"/>\n  <rect x="13.5" y="11" width="5" height="7" rx="1.2" fill="#fff"/>',
    'bin': '  <rect x="3.5" y="3.5" width="25" height="25" rx="5" fill="none" stroke="#78909C" stroke-width="2.4"/>\n' + ''.join(f'  <rect x="{9 + 5.5 * (i % 3)}" y="{9 + 5.5 * (i // 3)}" width="3.5" height="3.5" rx=".8" fill="#78909C" opacity="{o}"/>\n' for i, o in enumerate([1, .4, 1, .4, 1, 1, 1, 1, .4])).rstrip(),
}
os.makedirs(OUT, exist_ok=True)
for name, body in icons.items():
    open(os.path.join(OUT, name + '.svg'), 'w').write(svg(body))
print(len(icons))
