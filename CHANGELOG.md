# Changelog

What changed in each version of PDFRivet. The section for a version is also shown on its
GitHub release and in the app's update dialog, so write it for users, not developers.
The format follows [Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## [Unreleased]

### Look and feel
- Smooth, quick animations: menus, dialogs, tooltips, the find bar, the sidebar, tabs and annotation menus fade and glide in and out. Settings → Appearance → Animations turns them off (off from the start if your system asks for reduced motion).

### Text
- Select text with the mouse: drag, double-click a word, triple-click a line, Shift+click to extend, Ctrl+A for everything. Works the same in Arabic and other right-to-left text.
- Copy with Ctrl+C or the right-click menu. PDFs whose author doesn't allow copying say so.
- Arabic words with the lam-alef ligature (لا) are copied and found correctly.

### Search
- Text boxes (search, page number, comments) use PDFRivet's fonts, so Arabic shows properly there too.
- Find in document with Ctrl+F: results are highlighted on the pages as you type; Enter / Shift+Enter or F3 / Shift+F3 step through them.
- Arabic is found with or without diacritics, with any form of alef (أ إ آ ا), with ى or ي, and in old PDFs that store letters in their joined forms. Digits match in any script (١٢٣ finds 123).
- Match case and whole-word options.
- All results with the text around them in the sidebar's new Search tab.

### Annotations
- A new Annotate toolbar (A): highlight, underline and strike out text, draw with a pen, add rectangles, ellipses, lines, arrows and sticky notes, and erase drawings.
- Highlight, underline or strike out selected text from the right-click menu too.
- Pick each tool's colour, line width and opacity; rectangles and ellipses can be filled. Your choices are remembered.
- Click an annotation to select it: move it, resize it, change its colour, add a comment or delete it (Delete key).
- Undo and redo with Ctrl+Z and Ctrl+Y. Undoing a delete brings an annotation back exactly as it was, even one made in another app, and even after saving.
- Moving or resizing an annotation keeps its look (for example highlighter strokes made on an iPad keep their rounded, see-through style); while you drag, only the moved copy shows. Nothing blinks when you finish drawing, moving or highlighting.
- Highlight, underline and strike out on scanned pages and pictures: where there's no text, drag over the area.
- Annotations are saved the standard way, so other PDF readers show them too. Settings → General has the name shown as their author.

### Redaction
- Redact text or areas (Annotate → Redact): select text or drag over an area. Marked areas are shown outlined and can be unmarked; when you save (or choose Apply), they are blacked out and what was under them is removed from the file for good.
- A redacted page keeps its exact look, and its text outside the redacted areas can still be selected and searched. Its images and drawings become one picture of the page.
- Not available yet in password-protected PDFs.

### Signatures
- Sign documents: draw your signature with a mouse, pen or finger, or take it from a photo or scan of your signature on paper. The paper is removed automatically, with a slider to fine-tune it.
- Signatures can be saved for next time (only on your computer), placed with a click, then moved and resized.
- These are signatures you can see, not digital (certificate) signatures. Saving rewrites the file, so a document that already has a digital signature will no longer show it as valid.

## [0.1.0] – 2026-10-08

The first version: a fast, everyday PDF reader.

### Reading
- Open several PDFs in tabs: from the app, by drag and drop, or with "Open with" in your file manager.
- Continuous scrolling that stays fast on documents with thousands of pages.
- Single page or two pages side by side, with or without continuous scrolling. Two-page spreads follow the document's reading direction (right to left for Arabic, Persian, Hebrew…).
- Zoom to fit the page or the width, actual size, Ctrl + mouse wheel or pinch; rotate the view.
- Drag the page with the hand tool or the middle mouse button.
- Page thumbnails and the document's outline (table of contents).
- Clickable links, inside the document and to websites.
- Password-protected PDFs.
- Recent files reopen the way you left them: page, zoom, page display, scrolling and page colour; open them from the start screen or File → Open recent. Files that were moved or deleted are marked and can be removed from the list.

### Forms and saving
- Fill in forms: text fields, checkboxes, radio buttons and drop-down lists.
- Save, or save as a new file. Saving over the open file is safe: the file is replaced only once the new one is completely written.

### Printing
- PDFRivet's own print dialog with a live preview: choose the printer, copies, pages, scaling, paper size, orientation, two-sided printing and colour.
- Opens your printer's own settings window on Windows.

### Document properties
- See a document's details (pages, page size, PDF version, creator, security) and edit its title, author, subject and keywords.

### Everything else
- Updates install from inside the app (checked once a day; you can turn this off).
- A multilingual interface that fully mirrors for right-to-left languages.
- Light, dark and pure black themes (or follow the system).
- A Settings window (Ctrl+,) for language, themes, page colour, updates, and how documents open: zoom, single or two pages, continuous scrolling, tabs or separate windows.
- Open documents as tabs (default) or each in its own window.
- Windows: every tab gets its own preview in the taskbar.
- Keyboard shortcuts work with any keyboard layout, including Arabic.
- Page colors for comfortable reading: warm paper, green, dimmed, or dark pages with light text. Only the screen changes, never the file.
