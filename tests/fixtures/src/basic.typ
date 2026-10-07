// Rivet test fixture: three pages with different sizes.
// Rebuild: typst compile basic.typ ../basic.pdf
#set document(title: "Rivet fixture: basic", author: "Rivet contributors")
#set page(paper: "a4")
= Page one (A4 portrait)
The quick brown fox jumps over the lazy dog.
#pagebreak()
#set page(paper: "us-letter")
= Page two (US Letter)
Searchable text: rivet-needle.
#pagebreak()
#set page(paper: "a4", flipped: true)
= Page three (A4 landscape)
