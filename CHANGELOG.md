# Changelog

What changed in each version of PDFRivet. The section for a version is also shown on its
GitHub release and in the app's update dialog, so write it for users, not developers.
The format follows [Keep a Changelog](https://keepachangelog.com), and versions follow
[Semantic Versioning](https://semver.org).

## [Unreleased]

### Pages
- Organize pages (Tools menu, or right-click a thumbnail): every page as a big thumbnail. Drag pages into a new order, turn them, delete or duplicate them, add blank pages or pages from another PDF, with undo while you work. Done applies everything at once, and Ctrl+Z undoes it all in one step. Works the same in right-to-left layouts.
- Right-click a thumbnail to turn that page, delete it, or extract it into a new file.
- Extract pages (Tools menu): save some pages as a new PDF, and optionally delete them from this one.
- Deleted pages really leave the saved file, as with redaction. Bookmarks follow their pages; bookmarks of deleted pages go.
- Turning pages here changes the file; the toolbar's rotate button still only turns your view.
- Merge PDFs (Tools menu or the start page): put files in order, take all or some pages of each, unlock password-protected ones, and get one PDF with a bookmark per file (each file's own bookmarks under it). Form fields keep working; fields with the same name in different files are renamed so they don't share a value.
- The start page has the tools too.
- Split (Tools menu): into a file every few pages, one per page range ("1-3, 4-10"), or one per bookmark (named after it). Files go next to the document or into a folder you choose, and never overwrite existing ones.
- Extracted and split files keep the document's title and author, and the bookmarks and form fields of their pages.

### Added
- PDFRivet offers to become your default PDF app when another app opens PDF files (once, after you open a file; "Not now" waits a week). Settings → General shows which app opens your PDFs. On Windows this opens PDFRivet's page in the Windows settings, and the installer's last page has a link to it.

### Changed
- Saved files name PDFRivet as their producer and get an updated modification date. The program the document was made in (Creator) is kept.
- Pinch to zoom on a touchpad zooms the pages.
- The interface's labels and buttons can no longer be selected like text.

### Fixed
- Titles, authors, bookmarks, comments and form fields that another app saved in the wrong text encoding (UTF-8 without a marker) no longer show as garbled letters like "Ø¬Ù‘Ù–Ù—": they show in their own language, Arabic or any other, in password-protected files too. Changing the document's description no longer saves such a garbled title back into the file.
- Opening PDFRivet (or a PDF with it) no longer flashes a white window, then the start page, before the document; with two screens, the window no longer appears on one screen and jumps to the other. It now appears once, where you left it, with the document in it.
- After jumping to a page with small pages in view, the page you jumped to is the one highlighted in the thumbnails.
- Shortcuts no longer act on the document behind an open dialog (Ctrl+A in Settings selected the page's text; Ctrl+O opened a file).

## [0.2.0] – 2026-10-10

PDFRivet now edits: select and search text, annotate, sign, write on pages, redact,
bookmark and comment, with Arabic and other right-to-left languages handled properly
everywhere.

### Text and search
- Select text with the mouse: drag, double-click a word, triple-click a line, Shift+click to extend, Ctrl+A for everything. Works the same in Arabic and other right-to-left text.
- Copy with Ctrl+C or the right-click menu (PDFs whose author doesn't allow copying say so). Arabic words with the lam-alef ligature (لا) are copied correctly.
- Find in document with Ctrl+F: results are highlighted as you type; Enter / Shift+Enter or F3 / Shift+F3 step through them. Match case and whole-word options.
- Arabic is found with or without diacritics, with any form of alef (أ إ آ ا), with ى or ي, and in old PDFs that store letters in their joined forms. Digits match in any script (١٢٣ finds 123).
- Every result with the text around it in the sidebar's Search tab.

### Annotations
- A new Annotate toolbar (A): highlight, underline and strike out text, draw with a pen, add rectangles, ellipses, lines, arrows and sticky notes, and erase drawings.
- Highlight, underline or strike out selected text from the right-click menu too; on scanned pages and pictures, drag over the area instead.
- Pick each tool's colour, line width and opacity; rectangles and ellipses can be filled. Your choices are remembered.
- Click an annotation to move it, resize it, change its colour, add a comment or delete it. Moving keeps its look (an iPad highlighter stroke stays rounded and see-through), and nothing blinks while you work.
- Undo and redo with Ctrl+Z and Ctrl+Y, even for deletions and even after saving.
- Annotations are saved the standard way, so other PDF readers show them too. Settings → General has the name shown as their author.

### Text boxes
- Write on any page with the Text tool (T): click and type. The box grows with your text; drag its side to set a width so lines wrap, or its bottom to make room and place the text at the top, middle or bottom. Double-click a box (or select it and press Enter) to edit it.
- Arabic is written properly: joined letters, ligatures, right-to-left lines, and Arabic and English mixed on one line in the right order.
- Choose the font (PDFRivet's own Rubik and Amiri, or any font on your computer), size (Ctrl+[ and Ctrl+] while typing), bold, colour, where the text sits in the box, and its direction (from the text, right to left, or left to right).
- Other readers show text boxes exactly as PDFRivet does, and Acrobat can edit them. Text boxes from other apps whose Arabic came out broken look right once edited in PDFRivet.

### Signatures
- Draw your signature with a mouse, pen or finger, or take it from a photo or scan of your signature on paper (the paper is removed automatically, with a slider to fine-tune it).
- Save signatures for next time (only on your computer), place them with a click, then move and resize them.

### Comments
- A comments panel (toolbar button or Ctrl+Shift+C) lists every annotation, page by page: highlights with the text they mark, drawings and shapes with a small picture, notes, text boxes and signatures. Click one to go to it.
- Write or change comments there and reply to them. Replies are saved the standard way, so Acrobat shows them under the comment.
- Search the comments, or show only some kinds of annotations or one author's.

### Bookmarks
- Bookmark the page you're on with Ctrl+B, the right-click menu or the sidebar's Bookmarks tab; selected text becomes its title.
- Rename (double-click or F2), delete, drag to reorder or nest, or move with Alt+arrow keys. Undo works here too.
- Bookmarks are saved in the PDF itself, so other readers show them. A document's existing bookmarks can be edited the same way.

### Redaction
- Annotate → Redact: select text or drag over an area. When you save (or choose Apply, after a confirmation), the areas are blacked out and what was under them is removed from the file for good.
- A redacted page keeps its exact look, and its other text can still be selected and searched.

### Look and feel
- Drag the sidebar's edge to make it wider (up to a quarter of the window); thumbnails grow with it, and the size slider fits 2, 3 or more per row.
- The + button (or Ctrl+T) opens a new tab with the start page.
- Pages you just saw, and the next page when reading a page at a time, appear instantly. In page-at-a-time mode the mouse wheel turns one page per notch.
- Smooth, quick animations for menus, dialogs, tooltips and panels; Settings → Appearance turns them off.
- Search boxes and other text fields show Arabic in PDFRivet's fonts.
- PDF files get their own icon in Explorer once PDFRivet is your default PDF app (Windows).

### Good to know
- Signatures are signatures you can see, not digital (certificate) signatures. Saving rewrites the file, so a document that already has a digital signature will no longer show it as valid.
- Redaction, editing bookmarks and replying to comments aren't available yet in password-protected PDFs.
- Redaction turns a page's images and drawings into one picture of the page; text on a line that touches a redacted area may no longer be selectable.
- A text box has one style for all its text. On pages rotated in the file, text boxes are written in the page's own orientation.

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
