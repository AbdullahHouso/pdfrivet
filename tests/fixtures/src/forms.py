"""Rivet test fixture: an interactive form (AcroForm).

Rebuild:  python3 -I forms.py ../forms.pdf   (needs: pip install reportlab)
"""

import sys

from reportlab.lib.pagesizes import A4
from reportlab.pdfgen import canvas

out = sys.argv[1] if len(sys.argv) > 1 else "forms.pdf"
c = canvas.Canvas(out, pagesize=A4)
c.setTitle("Rivet fixture: forms")
form = c.acroForm
_, height = A4

c.setFont("Helvetica-Bold", 18)
c.drawString(72, height - 72, "Form fields")
c.setFont("Helvetica", 12)

c.drawString(72, height - 120, "Name:")
form.textfield(name="name", x=160, y=height - 130, width=250, height=22, value="", borderWidth=1)

c.drawString(72, height - 170, "I agree:")
form.checkbox(name="agree", x=160, y=height - 176, size=18, checked=False, buttonStyle="check")

c.drawString(72, height - 220, "Colour:")
form.radio(name="colour", value="red", x=160, y=height - 226, size=18, selected=True, buttonStyle="circle")
c.drawString(184, height - 220, "Red")
form.radio(name="colour", value="blue", x=240, y=height - 226, size=18, selected=False, buttonStyle="circle")
c.drawString(264, height - 220, "Blue")

c.drawString(72, height - 270, "Country:")
form.choice(name="country", x=160, y=height - 280, width=150, height=22,
            options=["Saudi Arabia", "Egypt", "Jordan"], value="Egypt")

c.drawString(72, height - 320, "Locked:")
form.textfield(name="locked", x=160, y=height - 330, width=250, height=22, value="read only",
               fieldFlags="readOnly", borderWidth=1)

c.showPage()
c.save()
