// Rivet test fixture: an internal link (to page 2) and a web link.
// Rebuild: typst compile links.typ ../links.pdf
#set document(title: "Rivet fixture: links")
#set page(paper: "a4")
= Links
#link(<second>)[Go to page two]

#link("https://example.com")[Visit example.com]
#pagebreak()
= Page two <second>
You arrived on page two.
