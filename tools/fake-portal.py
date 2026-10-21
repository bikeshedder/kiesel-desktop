#!/usr/bin/env python3
"""A stand-in for xdg-desktop-portal's Settings interface on the session bus.

Serves settings the way the real portal does – Read, ReadOne, ReadAll and the
SettingChanged signal – so apps can follow the system in a headless session.
It starts with org.freedesktop.appearance color-scheme (0 = no preference,
1 = dark, 2 = light); kiesel-headless changes that and any other setting
(accent-color, contrast, reduced-motion, GNOME's text-scaling-factor…)
through the private org.kiesel.FakePortal.Set method.

    fake-portal.py <initial color-scheme: 0|1|2>
"""
import sys

from gi.repository import Gio, GLib

BUS_NAME = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"

XML = """
<node>
  <interface name="org.freedesktop.portal.Settings">
    <method name="ReadAll">
      <arg type="as" name="namespaces" direction="in"/>
      <arg type="a{sa{sv}}" name="value" direction="out"/>
    </method>
    <method name="Read">
      <arg type="s" name="namespace" direction="in"/>
      <arg type="s" name="key" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <method name="ReadOne">
      <arg type="s" name="namespace" direction="in"/>
      <arg type="s" name="key" direction="in"/>
      <arg type="v" name="value" direction="out"/>
    </method>
    <signal name="SettingChanged">
      <arg type="s" name="namespace"/>
      <arg type="s" name="key"/>
      <arg type="v" name="value"/>
    </signal>
    <property name="version" type="u" access="read"/>
  </interface>
  <interface name="org.kiesel.FakePortal">
    <method name="Set">
      <arg type="s" name="namespace" direction="in"/>
      <arg type="s" name="key" direction="in"/>
      <arg type="v" name="value" direction="in"/>
    </method>
  </interface>
</node>
"""

initial = int(sys.argv[1]) if len(sys.argv) > 1 else 0
settings = {("org.freedesktop.appearance", "color-scheme"): GLib.Variant("u", initial)}


def matches(ns, patterns):
    """ReadAll's namespaces: exact, or a trailing * for a prefix; none = all"""
    return not patterns or any(
        ns.startswith(p[:-1]) if p.endswith("*") else ns == p for p in patterns
    )


def on_call(conn, sender, path, interface, method, params, invocation):
    if method in ("ReadOne", "Read"):
        found = settings.get(tuple(params.unpack()))
        if found is not None:
            # the old Read wraps the value in one more variant
            value = found if method == "ReadOne" else GLib.Variant("v", found)
            invocation.return_value(GLib.Variant("(v)", (value,)))
            return
    elif method == "ReadAll":
        (patterns,) = params.unpack()
        value = {}
        for (ns, key), v in settings.items():
            if matches(ns, patterns):
                value.setdefault(ns, {})[key] = v
        invocation.return_value(GLib.Variant("(a{sa{sv}})", (value,)))
        return
    elif method == "Set":
        ns, key = params.get_child_value(0).get_string(), params.get_child_value(1).get_string()
        value = params.get_child_value(2).get_variant()
        settings[(ns, key)] = value
        conn.emit_signal(
            None, PATH, "org.freedesktop.portal.Settings", "SettingChanged",
            GLib.Variant("(ssv)", (ns, key, value)),
        )
        invocation.return_value(None)
        return
    invocation.return_dbus_error(
        "org.freedesktop.portal.Error.NotFound", f"Requested setting {params.unpack()} not found"
    )


def on_get_property(conn, sender, path, interface, name):
    return GLib.Variant("u", 2) if name == "version" else None


def on_bus(conn, name):
    global connection
    connection = conn
    node = Gio.DBusNodeInfo.new_for_xml(XML)
    for iface in node.interfaces:
        conn.register_object(PATH, iface, on_call, on_get_property, None)


def on_name_acquired(conn, name):
    print("ready", flush=True)


Gio.bus_own_name(
    Gio.BusType.SESSION, BUS_NAME, Gio.BusNameOwnerFlags.NONE, on_bus, on_name_acquired,
    lambda *_: sys.exit("could not own " + BUS_NAME),
)
GLib.MainLoop().run()
