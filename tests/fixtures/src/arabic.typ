// Rivet test fixture: Arabic (RTL) text mixed with English and numbers.
// Rebuild: typst compile --font-path <dir with an Arabic font> arabic.typ ../arabic.pdf
#set document(title: "ملف اختبار عربي", author: "Rivet contributors")
#set page(paper: "a4")
#set text(font: "FreeSerif", lang: "ar", size: 16pt)
= مرحبا بكم في ريفت
هذا ملف PDF للاختبار يحتوي على نص عربي وأرقام ١٢٣ و 456.

#set text(lang: "en")
Mixed line: Rivet يدعم العربية and English.
