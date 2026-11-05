#!/usr/bin/env python3
"""Render a preview of lightdm-gtk-greeter with the ValoLinux theme (no LightDM needed).

Loads the greeter's own .glade UI, applies our GTK theme + greeter CSS and the background,
and runs inside Xvfb. Dev tool only, not part of the image.

  xvfb-run -s "-screen 0 1920x1080x24" python3.12 tools/preview-greeter.py \
      <path/to/lightdm-gtk-greeter/src> <out.png> [error]

Needs python3-gi + gir1.2-gtk-3.0 and ImageMagick `import`.
"""
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHARE = ROOT / "image/config/includes.chroot/usr/share"
os.environ["XDG_DATA_DIRS"] = f"{SHARE}:/usr/share"

import gi  # noqa: E402
gi.require_version("Gtk", "3.0")
from gi.repository import GdkPixbuf, GLib, Gtk  # noqa: E402

src = pathlib.Path(sys.argv[1])
out = sys.argv[2]
show_error = len(sys.argv) > 3 and sys.argv[3] == "error"

settings = Gtk.Settings.get_default()
settings.set_property("gtk-theme-name", "ValoLinux")
settings.set_property("gtk-icon-theme-name", "Adwaita")
settings.set_property("gtk-font-name", "Inter 11")

screen = Gtk.Window().get_screen()
for css_file in ("lightdm-gtk-greeter-application.css",):
    p = Gtk.CssProvider()
    p.load_from_path(str(src / css_file))
    Gtk.StyleContext.add_provider_for_screen(screen, p, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
bg = Gtk.CssProvider()
bg.load_from_data(
    f'#preview_root {{ background-image: url("file://{SHARE}/backgrounds/valolinux-greeter.png");'
    " background-size: cover; }".encode()
)
Gtk.StyleContext.add_provider_for_screen(screen, bg, Gtk.STYLE_PROVIDER_PRIORITY_USER)

b = Gtk.Builder()
ui = (src / "lightdm-gtk-greeter.glade").read_text()
# GreeterMenuBar is a GtkMenuBar subclass from the greeter binary; plain GtkMenuBar is enough here.
b.add_from_string(ui.replace("GreeterMenuBar", "GtkMenuBar"))
overlay = b.get_object("screen_overlay")
login = b.get_object("login_window")
panel = b.get_object("panel_window")

store = b.get_object("user_liststore")
store.append(["valo", "valo", 400])
b.get_object("user_combobox").set_active(0)
b.get_object("user_combobox").show()
img = GdkPixbuf.Pixbuf.new_from_file_at_scale(str(SHARE / "pixmaps/valolinux-logo.png"), 80, 80, True)
b.get_object("user_image").set_from_pixbuf(img)
b.get_object("clock_menuitem").set_label("Donnerstag, 05. November  08:15")
b.get_object("clock_menuitem").show()
# show only what the ValoLinux greeter config enables (clock, session, power)
for name in ("host_menuitem", "layout_menuitem", "language_menuitem", "keyboard_menuitem", "a11y_menuitem"):
    w = b.get_object(name)
    if w:
        w.set_no_show_all(True)
        w.hide()
b.get_object("login_button").set_label("Anmelden")
b.get_object("cancel_button").set_label("Abbrechen")
b.get_object("password_entry").set_placeholder_text("Passwort")
b.get_object("password_entry").set_text("secret" if show_error else "")
infobar = b.get_object("greeter_infobar")
if show_error:
    b.get_object("message_label").set_text("Falsches Passwort, bitte nochmals versuchen.")
    infobar.set_message_type(Gtk.MessageType.ERROR)
    infobar.show_all()
    infobar.set_revealed(True) if hasattr(infobar, "set_revealed") else None
else:
    infobar.hide()

root = Gtk.Window()
root.set_name("preview_root")
root.set_default_size(1920, 1080)
root.set_decorated(False)
child = b.get_object("screen_overlay_child")
child.set_visible_window(False)
login.set_halign(Gtk.Align.CENTER)
login.set_valign(Gtk.Align.CENTER)
login.set_margin_top(108)  # position 55% vertically, like the greeter config
panel.set_halign(Gtk.Align.FILL)
panel.set_valign(Gtk.Align.START)
overlay.add_overlay(panel)
overlay.add_overlay(login)
root.add(overlay)
root.move(0, 0)
root.show_all()
if not show_error:
    infobar.hide()
b.get_object("username_entry").hide()
b.get_object("cancel_button").hide()


def shot():
    subprocess.run(["import", "-window", "root", out], check=True)
    Gtk.main_quit()
    return False


GLib.timeout_add(1500, shot)
Gtk.main()
