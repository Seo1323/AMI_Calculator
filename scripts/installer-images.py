#!/usr/bin/env python3
"""Renders assets/installer/*.svg to the 24-bit BMPs the NSIS installer needs (src-tauri/icons/installer-*.bmp).
Needs rsvg-convert and PyGObject with GdkPixbuf. Run from the repository root."""
import subprocess, tempfile, os
import gi
gi.require_version('GdkPixbuf', '2.0')
from gi.repository import GdkPixbuf

for name, w, h in (('sidebar', 164, 314), ('header', 150, 57)):
    with tempfile.TemporaryDirectory() as t:
        png = os.path.join(t, f'{name}.png')
        subprocess.run(['rsvg-convert', '-w', str(w), '-h', str(h), '-b', 'white', f'assets/installer/{name}.svg', '-o', png], check=True)
        pb = GdkPixbuf.Pixbuf.new_from_file(png)
        pb.savev(f'src-tauri/icons/installer-{name}.bmp', 'bmp', [], [])
        print(name, pb.get_width(), pb.get_height())
