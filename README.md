# Open McFlash (OMF)

A vector animation editor for the web modeled on **Macromedia Flash 8**: same layout, same keyboard shortcuts and same timeline logic. The core is written in Rust and compiles to WebAssembly; the interface is styled after Photopea.

The interface is in Spanish, like the Spanish edition of Flash 8. This README uses the official English Flash 8 names and gives the Spanish UI label in parentheses where it helps you find it, e.g. **Test Movie (Probar película)**.

> An independent clean-room reimplementation. It uses no Macromedia/Adobe code or assets.

**Live version:** the installable web version is published from the `gh-pages` branch to GitHub Pages, at `https://<user>.github.io/<repo>/`.

## Getting started

There is no backend: everything runs in the browser (Rust → WebAssembly).

- **To use it:** double-click `OMF.html`. It is a single file with everything embedded and works offline.
- **To develop:**

```powershell
# build the core (requires: rustup target add wasm32-unknown-unknown; cargo install wasm-pack)
wasm-pack build omf-core --release --target web --out-dir ../web/pkg --no-typescript

# regenerate OMF.html (and dist/, the installable version)
node build.js

# optional: serve web/ unbundled (http://localhost:8737); serve.js only serves static files
node serve.js

# optional: serve the installable version (PWA) at http://localhost:8737
node serve.js dist
```

### Installing it as an app (PWA, like Photopea)

`node build.js` also generates `dist/`: the same application as `index.html`, with `manifest.webmanifest`, icons and a service worker. When served over **https** (upload `dist/` to GitHub Pages, Netlify, Cloudflare Pages…) or from **http://localhost** (`node serve.js dist`), Edge and Chrome show an **Install** button in the address bar:

- it opens in its own window, with its icon in the Start menu, on the desktop or on the taskbar;
- it works offline (the first visit caches it, and each new version replaces the previous one);
- `.omf` files can be opened with it from the file explorer ("Open with").

Browsers do not install pages opened by double-click (`file://`), so `OMF.html` keeps working as always, without installing.

To put it online, just upload `dist/` (installable) or `OMF.html` to any static host.

## Tests

- **Core (Rust):** `cargo test --manifest-path omf-core/Cargo.toml`
- **End-to-end (Selenium + pytest, real Chrome):** `python -m pytest`
  - Tests `OMF.html` as if double-clicked (`file://`), so run `node build.js` first.
  - Mouse and keyboard input is native (WebDriver) on the canvas, menus, panels and timeline; results are checked by reading the editor state and the canvas pixels.
  - Each test starts with a clean application and the view at 100 %, aligned to whole pixels.
  - A test also fails if any error appears in the browser console.
  - `OMF_HEADED=1` shows the browser; `OMF_URL=http://localhost:8737/` tests the served version.
  - When a test fails, its screenshot is saved in `tests/e2e/artifacts/`.
  - Requirements: `pip install selenium pytest` and Chrome (Selenium downloads chromedriver by itself).

## Structure

| Path | Contents |
|---|---|
| `omf-core/src/model.rs` | Document, layers and keyframes; shapes (including Bézier paths); semantics of F5/F6/F7, Shift+F5/F6 and motion tweening |
| `omf-core/src/geom.rs` | Affine matrices (the same as SWF), curves, decomposition for tweening, hit-testing |
| `omf-core/src/paint.rs` | Brush/Eraser sweeps, region booleans (i_overlay), Paint Bucket flood fill, Pencil Straighten |
| `omf-core/src/tools.rs` | Behavior of each tool: Pen, Direct Selection, transform box, painting, overlays |
| `omf-core/src/timeline.rs` | Frame ranges (copy, paste, reverse, move, convert), guide/mask/folder layers, motion guides and scenes |
| `omf-core/src/morph.rs` | Shape tweening with shape hints |
| `omf-core/src/tl_editor.rs` | Timeline commands, special layers, onion skin and scenes exposed to the UI |
| `omf-core/src/grad.rs` | Linear and radial gradients (with focal point), stops, overflow and their transform box |
| `omf-core/src/fill_tools.rs` | Active fill, Lock Fill and the Fill Transform tool |
| `omf-core/src/text.rs` | Text fields: fonts (ttf-parser), layout, line wrapping, alignment and outlines |
| `omf-core/src/text_editor.rs` | Text tool: create, edit, format and break apart |
| `omf-core/src/shape_tools.rs` | PolyStar, Lasso and the Free Transform modes (skew, distort, envelope) |
| `omf-core/fonts/` | Arimo, Tinos and Cousine (SIL Open Font License, `OFL.txt`): the `_sans`, `_serif` and `_typewriter` fonts |
| `omf-core/src/bitmap.rs` | Imported images (PNG inside the document), their Library item and bitmap fills |
| `omf-core/src/as2/` | ActionScript 2 to AVM1 bytecode compiler: parser (`parse.rs`), code generator (`gen.rs`) and classes from .as files (`class.rs`) |
| `omf-core/src/actions.rs` | Actions panel: what is being edited, Check Syntax, errors and simple frame actions |
| `omf-core/src/sound.rs` | Imported sounds and frame sounds (Event, Start, Stop, Stream) |
| `omf-core/src/guides.rs` | Guides: drag, snap, lock, options; each symbol has its own |
| `omf-core/src/words.rs` | Document text for spell checking (fields, layers, labels, Library) |
| `omf-core/src/symbols.rs` | Symbols (graphic, button, movie clip), instances, color effects, blend modes, filters, 9-slice scaling and the Library |
| `omf-core/src/sym_editor.rs` | F8, Ctrl+F8, edit in place and in symbol-editing mode, Library operations and instance properties exposed to the UI |
| `omf-core/src/filters.rs` | Flash 8 filters (Drop Shadow, Blur, Glow, Bevel, Adjust Color) and per-pixel blend modes |
| `omf-core/src/render.rs` | Rasterization with tiny-skia (in Rust; the browser only copies pixels), with masks, instances, filters and blending |
| `omf-core/src/swf.rs` | SWF 8 exporter (DefineShape3, DefineSprite, DefineButton2, DefineScalingGrid and PlaceObject2/3), following the public specification |
| `omf-core/src/lib.rs` | `Editor` exposed to JS: state, view, undo, timeline, editing, file |
| `web/js/main.js` | Menus, toolbox, panels, shortcuts, dialogs, Test Movie |
| `web/js/timeline.js` | Timeline with Flash's visual grammar |
| `web/js/library.js` | Library, symbol dialogs, instance properties and Filters panel |
| `web/js/gradient.js` | Color Mixer gradient bar and gradient swatches |
| `web/js/actions.js` | Actions (F9) and Output (F2) panels, docked below the Stage |
| `web/js/audio.js` | Importing sounds (decoded by the browser) and playing them in the Library and during playback |
| `web/js/ruffle.js` | Loading of embedded Ruffle (no server needed) for Test Movie |
| `web/vendor/ruffle/` | Ruffle 0.7.1 (MIT / Apache-2.0), the variant without WebAssembly extensions, compressed |
| `web/js/rulers.js` | Stage rulers and new guides dragged out of them |
| `web/js/io.js` | File > Import, Publish Settings, Publish and HTML preview |
| `web/js/history.js` | History panel and Commands menu (replay steps, saved commands, run scripts) |
| `web/js/spell.js` | Check Spelling and its setup |
| `web/vendor/`, `web/dict/` | nspell (MIT) and Spanish and English Hunspell dictionaries (licenses next to each file) |
| `web/js/textui.js` | Text editing box, formatting in the Property inspector, Text menu and system fonts |

## Features

- **Flash 8 layout**: File…Help menus; two-column toolbox in the original order (Tools, View, Colors, Options); "Timeline / Scene 1 › Group / zoom" edit bar; timeline on top with eye/lock/outline columns.
- **Flash drawing model** (default):
  - Shapes are "raw": fill and stroke are separate pieces, and each side of a rectangle is its own segment.
  - Same color merges; a different color cuts away what is underneath; a new fill erases the lines it covers.
  - A line splits the fills it crosses and is split where it crosses other lines.
  - Whatever is moved or pasted floats on top and merges when deselected.
  - The selection rectangle cuts shapes along its edges.
  - **Object Drawing (J)**: shapes do not merge with each other.
  - Selected content is marked with Flash's dot pattern.
- **Groups and objects**: Group (Ctrl+G), Ungroup (Ctrl+Shift+G) and Break Apart (Ctrl+B, which turns objects into raw shapes). Double-clicking enters editing mode with everything else dimmed; exit with "←", with the "Scene 1 › Group" path, or by double-clicking outside.
- **Modify > Shape (Modificar > Forma)**: Smooth, Straighten, Optimize Curves, Convert Lines to Fills, Expand Fill and Soften Fill Edges.
- **Snapping**: to objects (magnet, with Flash's snap ring), to guides, to grid and to pixels. The grid can be shown and edited (color and spacing).
- **Rulers and guides**: View > Rulers (Ver > Reglas, Ctrl+Alt+Shift+R) shows pixel rulers with the pointer position. Horizontal and vertical guides are dragged out of them; they are moved with the arrow tool and deleted by dropping them on the ruler. View > Guides: Show Guides (Ctrl+;), Lock Guides (Ctrl+Alt+;), Edit Guides (color, snapping, locking and snap accuracy) and Clear Guides; Snap to Guides (Ctrl+Shift+;). Guides are saved with the document and each symbol has its own.
- **Tools**:
  - Selection (V):
    - A click selects a fill or a segment; a double-click selects everything connected.
    - Dragging an edge bends it; dragging a corner moves it. Fill and line move together.
    - Objects are only included in a marquee selection if they lie entirely inside it.
  - **Direct Selection (A)**, Photoshop-style: move anchors, handles and segments; Alt+click converts the point; Alt+drag breaks the handle; Delete removes anchors.
  - Free Transform (Q): the box rotates with the object and the transformation point (center circle) can be moved. Dragging a side between handles **skews**. Modifiers under Options (and under Modify > Transform): **Rotate and Skew**, **Scale**, **Distort** (corners move independently) and **Envelope** (a mesh of anchors and handles that bends the shape).
  - **Fill Transform (F)**: moves, narrows, scales and rotates a gradient and shifts its focal point; the pieces of a cut fill are edited together.
  - **Lasso (L)**: freehand or in **Polygon Mode** (double-click or Enter closes it); cuts raw shapes along its outline.
  - **Text (T)**: click = auto width, drag = fixed width with line wrapping. You type directly on the canvas with the same font that is later rendered (caret, selection, accents and system clipboard). **Static**, **dynamic** and **input** text (instance name, single line / multiline / multiline no wrap, selectable, border, variable and maximum characters). Font, size, color, bold, italic, alignment (including justify), letter spacing and line spacing, from the Property inspector and the **Text** menu (Texto, with its shortcuts). When part of the text is selected, font, size, style, color and spacing apply **only to that part** (several formats in one field). **Auto kern** using the font's kerning pairs. Ctrl+B breaks apart into letters (each keeping its format) and, a second time, into shapes. **More fonts**: system fonts (if the browser allows it) or a .ttf/.otf file; fonts used by any text are saved **inside the document**. Dynamic and input text can **embed font outlines** (Embed… / Incorporar…), and text with multiple formats is exported as HTML.
  - **PolyStar**: in the Rectangle tool's flyout (right-click or press and hold); polygon or star, 3 to 32 sides, star point size.
  - **Pen (P)**, Photoshop/Photopea-style: click = corner, drag = curve, clicking the first point closes the path, Enter/Esc finishes, Ctrl+Z removes the last point, and over a selected path it adds or removes points.
  - Line (N), Oval (O), Rectangle (R, with corner radius).
  - Pencil (Y): Straighten (recognizes lines, rectangles and ovals), Smooth and Ink modes.
  - Brush (B): creates real fills; Paint Normal, Paint Fills, Paint Behind, Paint Selection and Paint Inside modes; round, square, oval and diagonal tips.
  - Eraser (E): cuts fills and strokes; Erase Normal, Erase Fills, Erase Lines, Erase Selected Fills and Erase Inside modes; Faucet.
  - Paint Bucket (K): fills shapes or areas enclosed by lines, closing small, medium and large gaps.
  - Ink Bottle (S), Eyedropper (I), Hand (H/Space), Zoom (Z).
- **Timeline**:
  - Frames: F5, Shift+F5, F6, F7 and Shift+F6, also on ranges (drag across cells or Shift+click).
  - Dragging a selected keyframe moves it.
  - Cut, copy and paste frames (Ctrl+Alt+X/C/V), remove frames (Alt+Backspace), select all frames (Ctrl+Alt+A), reverse frames and convert to keyframes or blank keyframes.
  - **Motion tween**: rotate auto, none, CW or CCW with N times; scale; orient to path; snap; **Sync** (graphics follow their animation along the tween); easing and **custom easing** (curve editor).
  - **Shape tween**: matches corners and contours; **shape hints** (Ctrl+Shift+H), dragged onto the edge and removed by dragging them off the Stage.
  - Editing inside a tween creates a keyframe, as in Flash; a broken tween is shown dashed.
  - **Frame labels**: name (red flag), comment (//) and anchor.
  - **Onion skin**: draggable markers on the ruler, Onion 2 / Onion 5 / Onion All presets, onion skin outlines and **Edit Multiple Frames** (select and edit shapes across all keyframes in the range).
  - **Frame view**: Tiny, Small, Normal, Medium and Large; Short; and Preview with thumbnails.
- **Layers**:
  - Add, delete, rename by double-clicking, reorder by dragging, hide, lock and view as outlines.
  - **Guide**, **motion guide** (guided layers follow its path), **mask** (clips its layers when locked, as in Flash, and always on export) and collapsible **folders**.
  - **Layer Properties**: name, type, outline color and height; opened by double-clicking the layer icon.
- **Scenes**: Scene panel (Shift+F2) to add, duplicate, rename, reorder, delete and switch scenes, plus the Edit Scene button on the edit bar. Test Movie and the SWF play all scenes in sequence.
- **Symbols and Library**:
  - **Convert to Symbol (F8)**: name, type (movie clip, button or graphic), registration point on the 3×3 grid and 9-slice scaling guides. **New Symbol (Ctrl+F8)** opens an empty one.
  - **Library (Ctrl+L)**: preview (with playback for multi-frame items), folders, rename by double-clicking, duplicate, change type, properties and delete (warns if there are instances and removes them too, like Flash). Drag items onto the Stage to create instances, or onto a folder to organize them.
  - **Editing**: double-clicking an instance edits **in place** (everything else dimmed); double-clicking in the Library or Ctrl+E edits in **symbol-editing mode**, with the registration point at the center. Exit with the "Scene 1 › Symbol › Group" path, with "←", with Ctrl+E or by double-clicking outside.
  - **Instance properties**: behavior type, instance name, Swap Symbol, color effect (Brightness, Tint, Alpha and Advanced), graphic playback (Loop, Play Once, Single Frame and first frame), blend mode and Use Runtime Bitmap Caching.
  - **Filters** (Flash 8 panel): Drop Shadow, Blur, Glow, Bevel and Adjust Color, with their parameters, enable/disable and ordering; they are interpolated in motion tweens.
  - **Graphics** follow the timeline of their container; **movie clips** have their own; **buttons** have the Up, Over, Down and Hit states (labeled on the timeline), respond to the mouse in Test Movie and, with **Control > Enable Simple Buttons** (Control > Habilitar botones simples), on the Stage.
  - Ctrl+B breaks an instance apart into its contents, with the color effect applied.
  - In the SWF: movie clips as DefineSprite, buttons as DefineButton2, instance names, color effects, filters, blending, bitmap caching and 9-slice scaling.
- **Editing**: undo/redo, cut/copy, paste in center of view, Paste in Place, duplicate, arrange, flip, rotate 90°, align, nudge with arrow keys.
- **Color**: Flash's web-safe swatches, mixer with alpha, "no color", swap colors, black and white.
- **Gradients**: None / Solid / Linear / Radial types in the Color Mixer, a bar with up to 15 colors (click to add, drag down to remove), overflow (extend, reflect, repeat), **linear RGB** interpolation, the Flash 8 palette gradients and **Lock Fill** for the Paint Bucket and Brush. They are cut and merged by the drawing model like any other fill, tweened, and exported to the SWF (including focal radial gradients).
- **File**: save/open `.omf` (JSON), export PNG and **SWF** (plays in Ruffle/Flash Player; includes hidden layers like Flash 8, configurable in Document), local autosave.
  - **Import** (Ctrl+R to the Stage, or to the Library; also by dropping files on the Stage): PNG, JPEG, GIF, BMP or WebP. The image is stored in the Library as a bitmap (properties: name, size, smoothing and **Lossless or Photo (JPEG)** compression with quality and the resulting size), can be dragged onto the Stage, swapped for another, and Ctrl+B turns it into a shape with a bitmap fill.
  - **Bitmap fill**: "Bitmap" type (Mapa de bits) in the Color Mixer, tiled, used with the Paint Bucket and Brush, and editable with Fill Transform (move, width, height, size and rotation). In the SWF they are written as DefineBitsLossless2 or DefineBitsJPEG2/3. Instance color effects also tint the images they contain.
  - **Publish Settings** (Configuración de publicación, Ctrl+Shift+F12) and **Publish** (Shift+F12): SWF, HTML and PNG formats with their own file names; Compress movie (on by default, as in Flash 8) and Protect from import; HTML with dimensions, Paused at start, Loop, Display menu and **Ruffle** so the SWF plays in today's browsers. **Publish Preview > HTML** (Ctrl+F12) opens that page with the movie inside.
- **Commands**: **History** panel (Ctrl+F10) with each step and its **slider** (dragging it undoes or redoes up to that step; undone steps are dimmed); steps can be replayed and saved as a command that appears in the Commands menu (Manage Saved Commands: rename and delete). **Run Command…** opens saved steps (.json) or a script (.js/.jsfl) that receives `omf` and `ed`.
- **Check Spelling** (Text menu): offline Spanish and English, suggestions, Change / Change All / Ignore / Ignore All / Add to Personal; checks text fields, frame labels and, if configured, layer and Library names.
- **Control**: Enter plays, `,` `.` step through frames, Ctrl+Enter Test Movie.
- **ActionScript 2** (**Actions** panel, F9):
  - Frame actions (the "a" on the timeline), button actions (`on (release) {…}`, `press`, `rollOver`, `keyPress "<Enter>"`…) and movie clip actions (`onClipEvent (load | enterFrame | mouseDown…) {…}` and `on (…)`).
  - The built-in compiler translates the Flash 8 language (variables, functions, objects, arrays, `if`, `for`, `for…in`, `while`, `switch`, `with`, type annotations…) into AVM1 bytecode, with the classic actions (`stop`, `play`, `gotoAndPlay` with labels and scenes, `getURL`, `trace`, `startDrag`, `duplicateMovieClip`…).
  - `try`/`catch`/`finally`, `throw`, `import` and `#include "file.as"`.
  - **ActionScript files (.as)** stored in the document (panel drop-down, "New .as…" / «Nuevo .as…»): **AS 2.0 classes** with packages, `extends`, `super`, static members, `get`/`set` and member names without `this.`, compiled into the init actions of the first frame.
  - Symbol **Linkage** (Library > Linkage… / Biblioteca > Vinculación…): identifier for `attachMovie` and AS 2.0 class (`Object.registerClass`), with ExportAssets in the SWF.
  - **Syntax highlighting**; Check Syntax marks the line with the error (also in .as files); "+ Add" menu with the most common actions.
- **Test Movie** (Probar película, Ctrl+Enter) plays the SWF in **Ruffle**, bundled in `OMF.html` (loaded on first use, offline), with `trace()` output and compile errors in the **Output** panel (Salida, F2), in Flash's format ("**Error** Scene=…, layer=…, frame=…: Line N: …"). If Ruffle cannot be loaded, the built-in player is used.
- **Control > Enable Simple Frame Actions** (Control > Habilitar acciones de fotograma simples, Ctrl+Alt+F): during playback in the editor, `stop()`, `play()`, `gotoAndPlay/Stop` and `nextFrame/prevFrame` are obeyed.
- **Sound**: File > Import accepts WAV, MP3, OGG, AAC, FLAC… (decoded by the browser; stored at 22 kHz, 16-bit). In the Library you can listen to them and see their waveform. In the Property inspector, each keyframe chooses a sound, sync (Event, Start, Stop, Stream) and repeat count; the timeline draws the waveform and it plays during playback. **Effects**: left/right channel, fade left to right / right to left, fade in, fade out and **Edit Envelope** (per-channel handles and in/out points). **Compression** in Sound Properties: Raw or **ADPCM** from 2 to 5 bits (with Test). In the SWF: DefineSound + StartSound (with the envelope in SOUNDINFO), and SoundStreamHead2 + per-frame blocks for streaming.
- **Full screen** (View or Ctrl+Alt+F): there, Ctrl+N, Ctrl+W and Ctrl+T work, which the browser normally reserves for itself.

## Status

`OMF.html` is about 12 MB: the core, Ruffle, three fonts and the dictionaries are embedded so it works offline.

All blocks of the plan are done.

Known limitations:
- ActionScript: in the editor, actions only run with Enable Simple Frame Actions; everything else is visible in Test Movie. Class interfaces are accepted but not checked, and only one `catch` per `try` is supported.
- Sound: no MP3 compression (only Raw and ADPCM). Several stream sounds playing at once are mixed into one.
- Movie clips show their first frame in the editor and, in the built-in player, advance at the main movie's pace (in the SWF they play independently, as in Flash).
- After cuts and merges, shapes are rebuilt with refitted curves (maximum error 0.15 px).

## License

Open McFlash is licensed under the GNU General Public License v3.0 or later (see `LICENSE`).

Third-party components keep their own licenses:

- Ruffle 0.7.1 — MIT / Apache-2.0 (`web/vendor/ruffle/`, see `LICENSE_MIT` and `LICENSE_APACHE`).
- nspell — MIT (`web/vendor/nspell.LICENSE`).
- Hunspell dictionaries in `web/dict/` — licenses next to each file (`es.LICENSE`, `en.LICENSE`).
- Arimo, Tinos and Cousine fonts — SIL Open Font License (`omf-core/fonts/OFL.txt`).
