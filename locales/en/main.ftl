## Rivet – English (source language)
## Every other language falls back to these strings.

app-name = Rivet
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
print = Print…
# Shown while pages are rendered for printing, e.g. "Preparing 3 of 15 pages for printing…"
print-title = Print
print-pages = Pages to print
print-current-page = Current page ({ $page })
print-all-pages = All pages ({ $count })
print-custom-pages = Pages:
print-range-example = e.g. 1-5, 8, 11-13
print-range-invalid = Use page numbers from 1 to { $count }, like 1-5, 8.
print-page-count = { $count ->
    [one] One page will be prepared.
   *[other] { $count } pages will be prepared.
}
print-many-pages = { $count } pages will be prepared. This can take a while; you can cancel at any time.
print-continue = Continue…
preparing-print = Preparing { $done } of { $total } pages for printing…
menu = Menu
theme = Theme
theme-system = System
theme-light = Light
theme-dark = Dark
language = Language
about = About Rivet

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
close-tab = Close { $name }

## Sidebar

thumbnails = Pages
outline = Outline
outline-empty = This document has no outline.
go-to-page-n = Go to page { $page }

## Start screen

drop-hint = …or drop PDF files anywhere in this window.
drop-to-open = Drop to open
recent-files = Recent files
file-missing = File not found
remove-from-recent = Remove { $name } from recent files
pdf-files = PDF documents

## Password dialog

password-title = Password required
password-prompt = “{ $name }” is protected. Enter its password to open it.
password = Password
cancel = Cancel

## About

version = Version { $version }
about-license = Free and open source under the Mozilla Public License 2.0.
close = Close

## Errors (keys match rivet-core ErrorCode)

error-library-not-found = The PDF engine (PDFium) could not be loaded. Please reinstall Rivet.
error-file-not-found = The file could not be found.
error-password-required = This PDF is protected with a password.
error-wrong-password = Wrong password. Please try again.
error-invalid-pdf = This file is damaged or is not a PDF.
error-page-out-of-range = That page does not exist.
error-document-not-open = The document is no longer open.
error-engine-stopped = The PDF engine stopped unexpectedly. Please restart Rivet.
error-cancelled = The request was cancelled.
error-read-only-field = This field can't be changed.
error-save-failed = The file could not be saved. Check that you can write to that folder, or use “Save as…”.
error-io = The file could not be read.
error-internal = Something went wrong.
dismiss = Dismiss
