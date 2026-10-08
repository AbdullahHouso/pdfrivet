# Changelog

What changed in each version of PDFRivet. The section for a version is also shown on its
GitHub release and in the app's update dialog, so write it for users, not developers.
The format follows [Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## [Unreleased]

### Text
- Select text with the mouse: drag, double-click a word, triple-click a line, Shift+click to extend, Ctrl+A for everything. Works the same in Arabic and other right-to-left text.
- Copy with Ctrl+C or the right-click menu. PDFs whose author doesn't allow copying say so.
- Arabic words with the lam-alef ligature (لا) are copied and found correctly.

### Search
- Find in document with Ctrl+F: results are highlighted on the pages as you type; Enter / Shift+Enter or F3 / Shift+F3 step through them.
- Arabic is found with or without diacritics, with any form of alef (أ إ آ ا), with ى or ي, and in old PDFs that store letters in their joined forms. Digits match in any script (١٢٣ finds 123).
- Match case and whole-word options.
- All results with the text around them in the sidebar's new Search tab.

### Annotations
- A new Annotate toolbar (A): highlight, underline and strike out text, draw with a pen, add rectangles, ellipses, lines, arrows and sticky notes, and erase drawings.
- Highlight, underline or strike out selected text from the right-click menu too.
- Pick each tool's colour, line width and opacity; rectangles and ellipses can be filled. Your choices are remembered.
- Click an annotation to select it: move it, resize it, change its colour, add a comment or delete it (Delete key).
- Undo and redo with Ctrl+Z and Ctrl+Y.
- Annotations are saved the standard way, so other PDF readers show them too. Settings → General has the name shown as their author.

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
