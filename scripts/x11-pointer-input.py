#!/usr/bin/env python3
"""Center or move the X11 pointer for headed product evidence."""

import ctypes
import ctypes.util
import sys


x11 = ctypes.CDLL(ctypes.util.find_library("X11"))
xtst = ctypes.CDLL(ctypes.util.find_library("Xtst"))
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XDefaultScreen.argtypes = [ctypes.c_void_p]
x11.XDisplayWidth.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XDisplayHeight.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XWarpPointer.argtypes = [
    ctypes.c_void_p,
    ctypes.c_ulong,
    ctypes.c_ulong,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_uint,
    ctypes.c_uint,
    ctypes.c_int,
    ctypes.c_int,
]
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
xtst.XTestFakeRelativeMotionEvent.argtypes = [
    ctypes.c_void_p,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_ulong,
]
xtst.XTestFakeButtonEvent.argtypes = [
    ctypes.c_void_p,
    ctypes.c_uint,
    ctypes.c_int,
    ctypes.c_ulong,
]

display = x11.XOpenDisplay(None)
if not display:
    sys.exit("could not open X11 display")

if sys.argv[1:] == ["center"]:
    screen = x11.XDefaultScreen(display)
    root = x11.XDefaultRootWindow(display)
    x = x11.XDisplayWidth(display, screen) // 2
    y = x11.XDisplayHeight(display, screen) // 2
    result = x11.XWarpPointer(display, 0, root, 0, 0, 0, 0, x, y)
elif len(sys.argv) == 4 and sys.argv[1] == "move":
    try:
        delta_x = int(sys.argv[2])
        delta_y = int(sys.argv[3])
    except ValueError:
        sys.exit("move deltas must be integers")
    if abs(delta_x) > 40 or abs(delta_y) > 40:
        sys.exit("move deltas must stay within 40 units per axis")
    result = xtst.XTestFakeRelativeMotionEvent(display, delta_x, delta_y, 0)
elif len(sys.argv) == 3 and sys.argv[1] == "button":
    if sys.argv[2] not in ("down", "up"):
        sys.exit("button state must be down or up")
    result = xtst.XTestFakeButtonEvent(display, 1, sys.argv[2] == "down", 0)
else:
    sys.exit("usage: x11-pointer-input.py center | move DX DY | button down|up")

if not result:
    x11.XCloseDisplay(display)
    sys.exit("X11 pointer input failed")
x11.XSync(display, False)
x11.XCloseDisplay(display)
