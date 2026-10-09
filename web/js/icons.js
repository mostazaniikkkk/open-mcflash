// Custom SVG icons (20×20 strokes) for the Tools panel.
const s = (body) => `<svg viewBox="0 0 20 20">${body}</svg>`;

export const ICONS = {
  select: s('<path class="f" d="M5 2.5v14l3.6-3.4 2.4 5.3 2.2-1-2.4-5.2 4.9-.3z"/>'),
  transform: s('<rect x="4.5" y="4.5" width="11" height="11" stroke-dasharray="2 1.6"/><rect class="f" x="2.5" y="2.5" width="4" height="4"/><rect class="f" x="13.5" y="2.5" width="4" height="4"/><rect class="f" x="2.5" y="13.5" width="4" height="4"/><rect class="f" x="13.5" y="13.5" width="4" height="4"/>'),
  line: s('<path d="M4 16L16 4"/>'),
  pencil: s('<path d="M3.5 16.5l1-4L13 4l3 3-8.5 8.5z"/><path d="M11 6l3 3"/>'),
  brush: s('<path d="M16.5 3.5c-2 1.2-5.2 4.2-7 6.6l1.6 1.4c2.3-2 5.1-5.3 5.4-8z"/><path class="f" d="M8.6 11c-2 0-3.2 1.5-3.4 3.2-.1 1-1 1.8-2 2 2.8 1 6.6.3 7-2.6z"/>'),
  rect: s('<rect x="3" y="5" width="14" height="10"/>'),
  oval: s('<ellipse cx="10" cy="10" rx="7" ry="5.5"/>'),
  bucket: s('<path d="M4 9l5.5-5.5 6 6L10 15z"/><path d="M4 9h11.5"/><path class="f" d="M16.6 12.2c.9 1.4 1.4 2.3 1.4 3a1.4 1.4 0 01-2.8 0c0-.7.5-1.6 1.4-3z"/>'),
  ink: s('<path d="M7 3h6v3l2 2v9H5V8l2-2z"/><path d="M7 6h6"/><path class="f" d="M8 11h4v4H8z"/>'),
  eyedropper: s('<path d="M14.3 3.2a1.9 1.9 0 012.6 2.6l-2.2 2.2-2.6-2.6z"/><path d="M12.6 6.4L5 14l-1 2.9L7 16l7.6-7.6"/>'),
  eraser: s('<path d="M7.5 16.5L3 12l8-8 6 6-6.5 6.5z"/><path d="M7 8l5 5"/><path d="M7.5 16.5h9"/>'),
  hand: s('<path d="M6.5 11V5.2a1.2 1.2 0 012.4 0V9.5V3.8a1.2 1.2 0 012.4 0V9.5V4.6a1.2 1.2 0 012.4 0v5.6V7a1.2 1.2 0 012.3 0v5.5c0 3-2 5-5 5H10c-2 0-3-.8-4.2-2.4L3.6 12a1.2 1.2 0 011.9-1.4z"/>'),
  zoom: s('<circle cx="8.5" cy="8.5" r="5"/><path d="M12.2 12.2L17 17M6.5 8.5h4M8.5 6.5v4"/>'),
  swap: s('<path d="M5 8V5h8M11 3l2 2-2 2M15 12v3H7M9 17l-2-2 2-2"/>'),
  bw: s('<rect x="3" y="3" width="9" height="9" fill="#000" stroke="#ccc"/><rect x="8" y="8" width="9" height="9" fill="#fff" stroke="#888"/>'),
  noColor: s('<rect x="3" y="3" width="14" height="14" fill="#fff" stroke="#888"/><path d="M3.5 16.5l13-13" stroke="#e01b1b" stroke-width="2"/>'),
  pen: s('<path d="M10 2l4 7-4 9-4-9z"/><circle cx="10" cy="9" r="1.2"/>'),
  strokeLbl: s('<path d="M2 9L9 2"/>'),
  fillLbl: s('<path class="f" d="M2 7l4-4 4 4-4 3z" fill="currentColor"/>'),
  alignL: s('<path d="M3 2v16"/><rect x="5" y="5" width="9" height="3"/><rect x="5" y="11" width="6" height="3"/>'),
  alignHC: s('<path d="M10 2v16"/><rect x="5" y="5" width="10" height="3"/><rect x="7" y="11" width="6" height="3"/>'),
  alignR: s('<path d="M17 2v16"/><rect x="6" y="5" width="9" height="3"/><rect x="9" y="11" width="6" height="3"/>'),
  alignT: s('<path d="M2 3h16"/><rect x="5" y="5" width="3" height="9"/><rect x="11" y="5" width="3" height="6"/>'),
  alignVC: s('<path d="M2 10h16"/><rect x="5" y="5" width="3" height="10"/><rect x="11" y="7" width="3" height="6"/>'),
  alignB: s('<path d="M2 17h16"/><rect x="5" y="6" width="3" height="9"/><rect x="11" y="9" width="3" height="6"/>'),
  layer: s('<path d="M3 6.5L10 3l7 3.5-7 3.5z"/><path d="M3 10.5l7 3.5 7-3.5"/>'),
  editPencil: s('<path d="M3 15l1-3 7-7 2 2-7 7z"/>'),
};

// Custom cursors so the active tool shows, as in Flash.
const cur = (svg, x, y, fallback) =>
  `url("data:image/svg+xml,${encodeURIComponent(`<svg xmlns='http://www.w3.org/2000/svg' width='24' height='24' viewBox='0 0 24 24'>${svg}</svg>`)}") ${x} ${y}, ${fallback}`;

// Photoshop-style Pen: nib + indicator (* new, + add, − remove, o close).
function penCursors() {
  const nib = "<path d='M3 21l2.5-8.5L13 5l6 6-7.5 7.5z' fill='white' stroke='black' stroke-width='1.3' stroke-linejoin='round'/><path d='M3 21l5.2-5.2' stroke='black' stroke-width='1.1'/><circle cx='9' cy='15' r='1.4' fill='black'/>";
  const badge = {
    pen: '',
    'pen-new': "<path d='M18 2v6M15 5h6M16 3l4 4M20 3l-4 4' stroke='black' stroke-width='1.2'/>",
    'pen-add': "<path d='M19 1v7M15.5 4.5h7' stroke='white' stroke-width='3.4'/><path d='M19 1v7M15.5 4.5h7' stroke='black' stroke-width='1.4'/>",
    'pen-remove': "<path d='M15.5 4.5h7' stroke='white' stroke-width='3.4'/><path d='M15.5 4.5h7' stroke='black' stroke-width='1.4'/>",
    'pen-close': "<circle cx='19' cy='5' r='2.6' fill='white' stroke='black' stroke-width='1.3'/>",
  };
  return Object.fromEntries(Object.entries(badge).map(([k, b]) => [k, cur(nib + b, 3, 21, 'crosshair')]));
}

export const CURSORS = {
  rotate: cur("<path d='M6 12a6 6 0 1 1 3 5.2' fill='none' stroke='white' stroke-width='4' stroke-linecap='round'/><path d='M6 12a6 6 0 1 1 3 5.2' fill='none' stroke='black' stroke-width='1.6' stroke-linecap='round'/><path d='M2 10l4 4 4-4z' stroke='white' fill='black'/>", 12, 12, 'alias'),
  bucket: cur("<g stroke='white' stroke-width='3' fill='none'><path d='M5 13l6-6 7 7-6 6z'/></g><path d='M5 13l6-6 7 7-6 6z' fill='none' stroke='black' stroke-width='1.4'/><path d='M3 20c0-2 2-4 2-4s2 2 2 4a2 2 0 0 1-4 0z' fill='black' stroke='white'/>", 4, 21, 'crosshair'),
  ink: cur("<path d='M9 3h7v4l2 2v12H7V9l2-2z' fill='white' stroke='black' stroke-width='1.3'/><path d='M3 21l3-3' stroke='black' stroke-width='2'/>", 3, 21, 'crosshair'),
  eyedropper: cur("<path d='M17 3.5a2.4 2.4 0 0 1 3.4 3.4L18 9.3 14.7 6z' fill='black' stroke='white'/><path d='M15 8L5 18l-1.5 3.5L7 20l10-10' fill='white' stroke='black' stroke-width='1.3'/>", 3, 21, 'crosshair'),
  ...penCursors(),
  direct: cur("<path d='M5 2v17l4.3-4 2.9 6.4 2.6-1.2-2.9-6.2 5.9-.4z' fill='white' stroke='black' stroke-width='1.3' stroke-linejoin='round'/>", 5, 2, 'default'),
  'direct-point': cur("<path d='M5 2v17l4.3-4 2.9 6.4 2.6-1.2-2.9-6.2 5.9-.4z' fill='white' stroke='black' stroke-width='1.3' stroke-linejoin='round'/><rect x='15' y='16' width='6' height='6' fill='white' stroke='black'/>", 5, 2, 'default'),
  // Flash cursors next to the arrow: curve (bend) and corner.
  bend: cur("<path d='M3 2v14l3.5-3.3 2.3 5.2 2.1-.9-2.3-5.1 4.8-.3z' fill='black' stroke='white' stroke-width='1'/><path d='M13 21c2-4 5-5 9-5' fill='none' stroke='white' stroke-width='3.5'/><path d='M13 21c2-4 5-5 9-5' fill='none' stroke='black' stroke-width='1.5'/>", 3, 2, 'default'),
  corner: cur("<path d='M3 2v14l3.5-3.3 2.3 5.2 2.1-.9-2.3-5.1 4.8-.3z' fill='black' stroke='white' stroke-width='1'/><path d='M13 22l4-6 5 6' fill='none' stroke='white' stroke-width='3.5'/><path d='M13 22l4-6 5 6' fill='none' stroke='black' stroke-width='1.5'/>", 3, 2, 'default'),
  pivot: cur("<circle cx='12' cy='12' r='5' fill='white' stroke='black' stroke-width='1.4'/><path d='M12 2v5M12 17v5M2 12h5M17 12h5' stroke='black' stroke-width='1.4'/>", 12, 12, 'move'),
  faucet: cur("<path d='M3 7h11v5H3z' fill='white' stroke='black' stroke-width='1.3'/><path d='M14 8h4a2 2 0 0 1 2 2v3' fill='none' stroke='black' stroke-width='1.6'/><path d='M8 3v4M5 3h6' stroke='black' stroke-width='1.6'/><path d='M20 16c1 1.4 1.4 2.3 1.4 3a1.4 1.4 0 0 1-2.8 0c0-.7.4-1.6 1.4-3z' fill='black'/>", 20, 21, 'pointer'),
  eraser: cur("<path d='M8 20l-5-5 10-10 7 7-8 8z' fill='white' stroke='black' stroke-width='1.3'/><path d='M8 10l6 6' stroke='black'/>", 6, 18, 'cell'),
};
