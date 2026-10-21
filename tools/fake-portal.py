#!/usr/bin/env python3
"""A stand-in for xdg-desktop-portal's Settings interface on the session bus.

Serves org.freedesktop.appearance color-scheme (0 = no preference, 1 = dark,
2 = light) the way the real portal does – Read, ReadOne, ReadAll and the
SettingChanged signal – so apps can follow "the system" in a headless
session. kiesel-headless starts it and changes the value through the private
org.kiesel.FakePortal.SetColorScheme method.

    fake-portal.py <initial: 0|1|2>
"""
import sys

from gi.repository import Gio, GLib

BUS_NAME = "org.freedesktop.portal.Desktop"
PATH = "/org/freedesktop/portal/desktop"
NAMESPACE, KEY = "org.freedesktop.appearance", "color-scheme"

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
    <method name="SetColorScheme">
      <arg type="u" name="value" direction="in"/>
    </method>
  </interface>
</node>
"""

scheme = int(sys.argv[1]) if len(sys.argv) > 1 else 0
connection = None


def on_call(conn, sender, path, interface, method, params, invocation):
    global scheme
    if method == "ReadOne":
        ns, key = params.unpack()
        if (ns, key) == (NAMESPACE, KEY):
            invocation.return_value(GLib.Variant("(v)", (GLib.Variant("u", scheme),)))
            return
    elif method == "Read":
        ns, key = params.unpack()
        if (ns, key) == (NAMESPACE, KEY):
            # the old Read wraps the value in one more variant
            inner = GLib.Variant("v", GLib.Variant("u", scheme))
            invocation.return_value(GLib.Variant("(v)", (inner,)))
            return
    elif method == "ReadAll":
        value = {NAMESPACE: {KEY: GLib.Variant("u", scheme)}}
        invocation.return_value(GLib.Variant("(a{sa{sv}})", (value,)))
        return
    elif method == "SetColorScheme":
        (scheme,) = params.unpack()
        conn.emit_signal(
            None, PATH, "org.freedesktop.portal.Settings", "SettingChanged",
            GLib.Variant("(ssv)", (NAMESPACE, KEY, GLib.Variant("u", scheme))),
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
