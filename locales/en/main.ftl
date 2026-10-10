## PDFRivet – English (source language)
## Every other language falls back to these strings.

# The name stays "PDFRivet" (Latin letters) in every language.
app-name = PDFRivet
app-tagline = A free, fast PDF reader and editor

## Toolbar

open-file = Open PDF…
open = Open
toggle-sidebar = Show or hide the sidebar
previous-page = Previous page
next-page = Next page
page-number = Page number
# Shown after the page number box, e.g. "of 12"
of-total = of { $total }
zoom = Zoom
zoom-in = Zoom in
zoom-out = Zoom out
# $percent is a whole number, e.g. 125
zoom-level = { $percent }%
fit-width = Fit width
fit-page = Fit page
rotate-view = Rotate view
actual-size = Actual size
mouse-mode = Mouse mode
tool-select = Select (V)
tool-hand = Hand: drag to move (H)
page-display = Page display
single-page = Single page
two-pages = Two pages
continuous-scrolling = Continuous scrolling
pages-rtl = Pages right to left
pages-rtl-hint = Two-page spreads start on the right, like Arabic books. PDFRivet sets this from the document's language.
print = Print…
# Shown while pages are rendered for printing, e.g. "Preparing 3 of 15 pages for printing…"
print-title = Print
print-pages = Pages to print
print-current-page = Current page ({ $page })
print-all-pages = All pages ({ $count })
print-custom-pages = Pages:
print-range-example = e.g. 1-5, 8, 11-13
print-range-invalid = Use page numbers from 1 to { $count }, like 1-5, 8.
printer = Printer
printers-loading = Looking for printers…
printers-none = No printers found. You can still use the system dialog.
printer-default = default
printer-properties = Printer properties…
copies = Copies
collate = Collate
grayscale = Grayscale
print-subset = Subset
subset-all = All pages in range
subset-odd = Odd pages only
subset-even = Even pages only
print-reverse = Reverse order
page-sizing = Page sizing
sizing-fit = Fit
sizing-shrink = Shrink oversized
sizing-actual = Actual size
sizing-custom = Custom scale
paper = Paper
paper-size = Paper size
orientation = Orientation
orientation-auto = Automatic
orientation-portrait = Portrait
orientation-landscape = Landscape
two-sided = Two-sided
duplex-off = Off
duplex-long = Flip on long edge
duplex-short = Flip on short edge
print-preview = Preview
# e.g. "Page 3 of 12" in the print preview
print-preview-position = { $index } / { $total }
print-scale = Scale { $percent }%
print-sheets = { $count ->
    [one] 1 sheet of paper
   *[other] { $count } sheets of paper
}
print-system-dialog = Use the system print dialog…
print-button = Print
printing-sending = Sending to the printer…
preparing-print = Preparing { $done } of { $total } pages for printing…
menu = Menu
theme = Theme
theme-system = System
theme-light = Light
theme-dark = Dark
theme-black = Black
page-tone = Page color
page-tone-original = Original
page-tone-warm = Warm
page-tone-green = Green
page-tone-dimmed = Dimmed
page-tone-dark = Dark
language = Language
open-documents-in = Open documents in
documents-tabs = Tabs
documents-windows = Separate windows
taskbar-tabs = Show each tab in the taskbar
about = About PDFRivet
settings-menu = Settings…
settings = Settings
settings-general = General
settings-appearance = Appearance
settings-reading = Reading
updates = Updates
check-now = Check now
page-tone-hint = Laid over the pages while you read; your files aren't changed. Each document remembers the colour you last used for it.
animations = Enable animations
reading-defaults-hint = How documents open the first time. After that, each file reopens the way you left it: page, zoom, page display and scrolling.

## Saving

save = Save
save-as = Save as…
dont-save = Don't save
unsaved-changes = Unsaved changes
unsaved-title = Save your changes?
unsaved-message = “{ $name }” has changes that aren't saved yet.
unsaved-message-many = { $count ->
    [one] One document has changes that aren't saved yet.
   *[other] { $count } documents have changes that aren't saved yet.
}

## Forms

form-field = Form field

## Tabs

open-documents = Open documents
new-tab = New tab
close-tab = Close { $name }

## Sidebar

thumbnails = Pages
thumbnail-size = Thumbnail size
thumbnails-smaller = Smaller thumbnails
thumbnails-bigger = Bigger thumbnails
resize-sidebar = Resize the sidebar
bookmarks-tab = Bookmarks
bookmarks = Bookmarks
bookmarks-empty = No bookmarks yet. Press Ctrl+B to bookmark the page you're on.
bookmarks-protected = Bookmarks can't be changed in password-protected PDFs yet.
bookmark-add = Add bookmark
bookmark-add-inside = Add bookmark inside
bookmark-rename = Rename
bookmark-delete = Delete bookmark
page-n = Page { $page }
go-to-page-n = Go to page { $page }

## Start screen

drop-hint = …or drop PDF files anywhere in this window.
drop-to-open = Drop to open
recent-files = Recent files
file-menu = File
open-recent = Open recent
no-recent-files = No recent files
clear-recent = Clear recent files
close-document = Close
file-not-found-title = File not found
file-not-found-message = “{ $name }” may have been moved, renamed or deleted.
remove-from-recent-short = Remove from recent files
file-missing = File not found
remove-from-recent = Remove { $name } from recent files
pdf-files = PDF documents

## Password dialog

password-title = Password required
password-prompt = “{ $name }” is protected. Enter its password to open it.
password = Password
cancel = Cancel

## Document properties

document-properties = Document properties…
document-properties-title = Document properties
props-description = Description
props-protected-note = This document is password-protected, so its description can't be changed yet.
props-title = Title
props-author = Author
props-subject = Subject
props-keywords = Keywords
props-creator = Created with
props-producer = PDF producer
props-created = Created
props-modified = Modified
props-file = File
props-file-name = File name
props-location = Location
show-in-folder = Show in folder
props-file-size = File size
props-pages = Pages
props-page-size = Page size
props-unit = Unit
props-pdf-version = PDF version
props-advanced = Advanced
props-tagged = Tagged PDF
props-protected = Password-protected
props-allowed = What this document allows
perm-print = Printing
perm-copy = Copying text and images
perm-modify = Editing pages
perm-fill-forms = Filling in forms
perm-annotate = Comments and annotations
yes = Yes
no = No
ok = OK

## About

version = Version { $version }
about-license = Free and open source under the Mozilla Public License 2.0.
website = Website
close = Close

## Text
copy-text = Copy
select-all = Select all

## Search
search-placeholder = Find in document
search-tab = Search
search-searching = Searching…
search-no-results = No results
search-count = { $current } of { $total }
search-results = { $total ->
    [one] 1 result
   *[other] { $total } results
}
search-page = Page { $page }
search-match-case = Match case
search-whole-word = Whole words only
search-previous = Previous result
search-next = Next result
search-show-all = All results
search-selection = Search for selected text
search-pane-empty = Search the document’s text. Arabic is found with or without diacritics, and digits in any script.

author-name = Your name
author-name-hint = Shown as the author of your comments and drawings. Leave empty to use your computer’s user name.
## Annotations
annotate = Annotate
annotate-tools = Annotation tools
annotate-close = Close annotation tools
annot-select = Select and move annotations
annot-hint = Pick a tool. Highlight, underline, strikeout and redact work on text you select, or on an area you drag over (for scans and pictures).
annot-highlight = Highlight
annot-underline = Underline
annot-strikeout = Strikeout
annot-pen = Pen
annot-rectangle = Rectangle
annot-ellipse = Ellipse
annot-line = Line
annot-arrow = Arrow
annot-note = Sticky note
annot-text = Text box (T)
text-placeholder = Type here
text-edit = Edit text
text-font = Font
text-font-search = Find a font
text-font-none = No font matches.
text-fonts-bundled = PDFRivet fonts
text-fonts-arabic = With Arabic letters
text-fonts-other = Other fonts
text-size = Font size
text-bold = Bold
text-align = Text across the box
text-align-auto = Start side (follows the text direction)
text-align-left = Left
text-align-center = Centre
text-align-right = Right
text-valign = Text down the box
text-valign-top = Top
text-valign-middle = Middle
text-valign-bottom = Bottom
text-dir = Text direction
text-dir-auto = Direction from the text
text-dir-rtl = Right to left
text-dir-ltr = Left to right
comment-kind-text = Text boxes
annot-eraser = Eraser (drawings and shapes)
annot-style = Style
annot-custom-color = Other colour…
annot-width = Width
annot-opacity = Opacity
annot-fill = Fill
annot-comment = Comment
annot-comment-placeholder = Write a comment…
annot-delete = Delete
signature = Signature
signature-new = New signature…
signature-new-title = New signature
signature-choose-button = Choose a picture…
signature-draw = Draw
signature-image = From a picture
signature-draw-here = Sign here
signature-ink = Ink colour
signature-ink-black = Black
signature-ink-blue = Blue
signature-ink-red = Red
signature-clear = Clear
signature-choose-picture = A photo or scan of your signature on white paper.
signature-picture-error = That picture couldn’t be opened. Try a PNG or JPEG file.
signature-picture-empty = Nothing left: move the slider to the right.
signature-background = Remove background
signature-background-hint = Move the slider until the paper is gone and the signature is complete.
signature-keep = Save this signature for next time
signature-use = Use signature
signature-delete = Delete this signature
annot-redact = Redact
redact-hint = Select text, or drag over an area. Marked areas are removed for good when you save.
redact-apply = { $count ->
    [one] Apply 1 redaction
   *[other] Apply { $count } redactions
}
redact-unmark = Remove this mark
redact-confirm-title = Apply redactions?
redact-confirm = { $count ->
    [one] The marked area will be blacked out and what’s under it removed from the file for good. This can’t be undone.
   *[other] The { $count } marked areas will be blacked out and what’s under them removed from the file for good. This can’t be undone.
}
redact-confirm-apply = Redact and save
redact-confirm-apply-now = Redact
undo = Undo
redo = Redo

## Errors (keys match rivet-core ErrorCode)

error-library-not-found = The PDF engine (PDFium) could not be loaded. Please reinstall PDFRivet.
error-file-not-found = The file could not be found.
error-password-required = This PDF is protected with a password.
error-wrong-password = Wrong password. Please try again.
error-invalid-pdf = This file is damaged or is not a PDF.
error-page-out-of-range = That page does not exist.
error-document-not-open = The document is no longer open.
error-engine-stopped = The PDF engine stopped unexpectedly. Please restart PDFRivet.
error-cancelled = The request was cancelled.
error-read-only-field = This field can't be changed.
error-save-failed = The file could not be saved. Check that you can write to that folder, or use “Save as…”.
error-copy-not-allowed = The author of this PDF doesn’t allow copying its text.
error-annotate-not-allowed = The author of this PDF doesn’t allow adding comments or drawings.
error-annotation-not-found = That annotation no longer exists.
error-read-only-annotation = This annotation can’t be changed, only deleted.
error-redact-protected = Redaction isn’t possible in password-protected PDFs yet.
error-io = The file could not be read.
error-internal = Something went wrong.
dismiss = Dismiss

## Updates
check-updates = Check for updates…
auto-update = Check for updates automatically
update-checking = Checking for updates…
update-up-to-date-title = PDFRivet is up to date
update-up-to-date = You have the latest version ({ $version }).
update-available-title = Update available
update-available = PDFRivet { $version } is available. You have { $current }.
update-notes = What's new
update-now = Update now
update-later = Later
update-skip = Skip this version
update-downloading = Downloading the update… { NUMBER($progress, style: "percent") }
update-downloading-unknown = Downloading the update…
update-installing = Installing… PDFRivet will restart in a moment.
update-check-failed = Couldn't check for updates
update-install-failed = Couldn't install the update
update-error-offline = Check your internet connection and try again.
update-error-unavailable = Update information isn't available right now. Please try again later.
update-error-other = Something went wrong. Please try again later.
try-again = Try again

## Comments panel
toggle-comments = Show or hide comments
resize-comments = Resize the comments panel
comments = Comments
comments-search = Search comments
comments-kinds = Show only
comments-author = Author
comments-all-authors = All authors
comments-loading = Reading annotations…
comments-empty = No annotations yet. Highlights, drawings, shapes and notes you add show here, with their comments.
comments-none-match = Nothing matches.
comments-clear-filter = Show all
comment-kind-highlight = Highlights
comment-kind-underline = Underlines
comment-kind-strikeout = Strikeouts
comment-kind-ink = Drawings
comment-kind-shape = Shapes
comment-kind-note = Notes
comment-kind-stamp = Signatures and stamps
comment-kind-other = Other annotations
comment-no-author = Unknown author
comment-edit = Edit comment
reply = Reply
reply-placeholder = Write a reply…
reply-edit = Edit reply
reply-delete = Delete reply
more-actions = More actions
just-now = just now

## Default PDF app
default-app = Default PDF app
default-app-title = Open PDFs with PDFRivet?
default-app-text = Your PDF files open in another app. Make PDFRivet the default to open them here.
default-app-make = Make default
default-app-not-now = Not now
default-app-never = Don’t ask again
default-app-windows-steps = In the Windows settings that opened, click .pdf and choose PDFRivet.
default-app-open-settings = Open settings again
default-app-is-default = PDFRivet opens your PDF files.
default-app-not-default = Your PDF files open in another app.
default-app-ask = Offer to make PDFRivet the default
