#!/usr/bin/env python3
"""Send one bounded relative pointer movement through X11 XTest."""

import ctypes
import ctypes.util
import sys


if len(sys.argv) != 3:
    sys.exit("usage: x11-send-relative-look.py DX DY")

try:
    delta_x = int(sys.argv[1])
    delta_y = int(sys.argv[2])
except ValueError:
    sys.exit("DX and DY must be integers")

if abs(delta_x) > 100 or abs(delta_y) > 100:
    sys.exit("relative pointer movement must stay within 100 units per axis")

x11 = ctypes.CDLL(ctypes.util.find_library("X11"))
xtst = ctypes.CDLL(ctypes.util.find_library("Xtst"))
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
xtst.XTestFakeRelativeMotionEvent.argtypes = [
    ctypes.c_void_p,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_ulong,
]
xtst.XTestFakeRelativeMotionEvent.restype = ctypes.c_int

display = x11.XOpenDisplay(None)
if not display:
    sys.exit("could not open X11 display")
if not xtst.XTestFakeRelativeMotionEvent(display, delta_x, delta_y, 0):
    x11.XCloseDisplay(display)
    sys.exit("XTest relative pointer movement failed")
x11.XSync(display, False)
x11.XCloseDisplay(display)
