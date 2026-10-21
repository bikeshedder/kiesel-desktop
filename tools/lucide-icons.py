#!/usr/bin/env python3
"""Takes the Lucide icons an app uses into Rust data, with Lucide's licence notice.

    tools/lucide-icons.py --out src/icons.rs save trash-2 settings
    tools/lucide-icons.py --out kiesel-ui/src/lucide.rs --path crate::icon sun moon-star …

Each icon becomes a `pub const` (`save` → `SAVE`, `trash-2` → `TRASH_2`) of
`kiesel_ui::icon::IconNode`, drawn by `kiesel_ui::Icon` and `Button::icon`.
The SVGs come from the lucide-static npm package at a pinned version, checked
against the registry's checksum and cached in ~/.cache/kiesel. Only the icons
named are written – an app carries what it shows, nothing more. The notice at
the top of the file is Lucide's licence (ISC), plus Feather's (MIT) when one
of the icons comes from Feather.
"""
import argparse
import base64
import hashlib
import io
import json
import os
import re
import sys
import tarfile
import textwrap
import urllib.request
import xml.etree.ElementTree as ET

VERSION = "1.58.0"
SVG = "{http://www.w3.org/2000/svg}"


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "kiesel-lucide-icons"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def package(version):
    """The lucide-static tarball, from the cache or the registry, checksum verified"""
    cache = os.path.join(os.path.expanduser("~/.cache/kiesel"), f"lucide-static-{version}.tgz")
    meta = json.loads(fetch(f"https://registry.npmjs.org/lucide-static/{version}"))
    algo, digest = meta["dist"]["integrity"].split("-", 1)
    if os.path.exists(cache):
        data = open(cache, "rb").read()
    else:
        data = fetch(meta["dist"]["tarball"])
    if base64.b64encode(hashlib.new(algo, data).digest()).decode() != digest:
        sys.exit(f"lucide-static {version}: the download doesn't match the registry's checksum")
    os.makedirs(os.path.dirname(cache), exist_ok=True)
    with open(cache, "wb") as f:
        f.write(data)
    return tarfile.open(fileobj=io.BytesIO(data))


def num(v):
    """A Rust f64 literal"""
    f = float(v)
    return repr(f) if f != int(f) else f"{int(f)}.0"


def points(s):
    vals = [float(v) for v in re.split(r"[\s,]+", s.strip()) if v]
    return ", ".join(f"({num(x)}, {num(y)})" for x, y in zip(vals[::2], vals[1::2]))


def part(el):
    tag, a = el.tag.removeprefix(SVG), el.attrib
    g = lambda k, d="0": num(a.get(k, d))
    if tag == "path":
        return f'Part::Path("{a["d"]}")'
    if tag == "circle":
        return f"Part::Circle {{ cx: {g('cx')}, cy: {g('cy')}, r: {g('r')} }}"
    if tag == "ellipse":
        return f"Part::Ellipse {{ cx: {g('cx')}, cy: {g('cy')}, rx: {g('rx')}, ry: {g('ry')} }}"
    if tag == "rect":
        r = a.get("rx", a.get("ry", "0"))
        return (f"Part::Rect {{ x: {g('x')}, y: {g('y')}, width: {g('width')}, "
                f"height: {g('height')}, r: {num(r)} }}")
    if tag == "line":
        return f"Part::Line {{ x1: {g('x1')}, y1: {g('y1')}, x2: {g('x2')}, y2: {g('y2')} }}"
    if tag in ("polyline", "polygon"):
        closed = "true" if tag == "polygon" else "false"
        return f"Part::Poly {{ points: &[{points(a['points'])}], closed: {closed} }}"
    sys.exit(f"unknown element <{tag}>")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True, help="the Rust file to write")
    ap.add_argument("--path", default="kiesel_ui::icon", help="where IconNode and Part are")
    ap.add_argument("--version", default=VERSION, help=f"lucide-static version (default {VERSION})")
    ap.add_argument("names", nargs="+", help="Lucide names, as on lucide.dev: trash-2, moon-star")
    args = ap.parse_args()

    tar = package(args.version)
    licence = tar.extractfile("package/LICENSE").read().decode()
    isc, _, rest = licence.partition("---")
    feather = set(re.search(r"Feather project:\s*\n\s*\n(.*?)\n", rest, re.S).group(1).split(", "))
    mit = rest[rest.index("The MIT License"):].strip() if "The MIT License" in rest else ""

    consts, idents = [], []
    for name in sorted(set(args.names)):
        try:
            svg = tar.extractfile(f"package/icons/{name}.svg").read()
        except KeyError:
            sys.exit(f"no Lucide icon '{name}' in lucide-static {args.version} (see lucide.dev)")
        parts = [part(el) for el in ET.fromstring(svg)]
        ident = re.sub(r"\W", "_", name.upper())
        body = "".join(f"    {p},\n" for p in parts)
        consts.append(f"/// Lucide `{name}`\npub const {ident}: IconNode = IconNode(&[\n{body}]);\n")
        idents.append(ident)

    used_feather = sorted(set(args.names) & feather)
    notice = isc.strip()
    if used_feather:
        names = textwrap.fill(f"{', '.join(used_feather)}: derived from the Feather project –", 96)
        notice += f"\n\n{names}\n\n{mit}"
    path = "" if args.path == "kiesel_ui::icon" else f" --path {args.path}"
    names = " ".join(sorted(set(args.names)))
    command = textwrap.fill(
        f"tools/lucide-icons.py --out {args.out}{path} {names}", 92, subsequent_indent="//     "
    )
    header = "\n".join(f"// {l}".rstrip() for l in notice.splitlines())
    out = (
        f"// Generated by tools/lucide-icons.py from lucide-static {args.version} – don't edit;\n"
        f"// run it again with other names instead:\n"
        f"//   {command}\n"
        f"//\n// Lucide's licence (https://lucide.dev):\n//\n{header}\n\n"
        "#![allow(dead_code)] // `ALL`, and icons the app stopped using\n\n"
        f"use {args.path}::{{IconNode, Part}};\n\n" + "\n".join(consts)
        + f"\n/// Every icon in this file\npub const ALL: &[IconNode] = &[{', '.join(idents)}];\n"
    )
    with open(args.out, "w") as f:
        f.write(out)
    print(f"{args.out}: {len(consts)} icons", file=sys.stderr)


if __name__ == "__main__":
    main()
