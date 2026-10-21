# Usage: kiesel-headless run python3 tools/webkit-baseline.py http://localhost:8765/  (serve kiesel/dist first)
# Opens the kiesel kitchensink in WebKitGTK (Tauri's Linux engine) and reports PSS of all its processes.
import gi, os, sys, subprocess
gi.require_version("Gtk", "3.0"); gi.require_version("WebKit2", "4.1")
from gi.repository import Gtk, WebKit2, GLib
url = sys.argv[1]
win = Gtk.Window(); win.set_default_size(1280, 800)
view = WebKit2.WebView(); win.add(view); win.connect("destroy", Gtk.main_quit); win.show_all()
pages = ["", "#/c/button", "#/c/select", "#/c/calendar"]
def pss(pid):
    try:
        for l in open(f"/proc/{pid}/smaps_rollup"):
            if l.startswith("Pss:"): return int(l.split()[1])
    except OSError: return 0
    return 0
def tree(root):
    out, todo = [], [root]
    while todo:
        p = todo.pop(); out.append(p)
        todo += [int(c) for c in os.listdir("/proc") if c.isdigit() and open(f"/proc/{c}/stat").read().rsplit(") ", 1)[1].split()[1] == str(p)] if os.path.exists(f"/proc/{p}") else []
    return out
def step():
    if pages:
        view.load_uri(url + pages.pop(0)); return True
    rows = [(open(f"/proc/{p}/comm").read().strip(), pss(p)) for p in tree(os.getpid())]
    for n, k in rows: print(f"  {n:24} {k/1024:7.1f} MiB PSS")
    print(f"  TOTAL {sum(k for _, k in rows)/1024:.1f} MiB PSS over {len(rows)} processes")
    Gtk.main_quit(); return False
GLib.timeout_add(2500, step)
Gtk.main()
