#!/usr/bin/env python3
"""Send a held pointer press plus Escape and Return to Loading Bay on X11."""

import ctypes
import ctypes.util
import sys
import time

x11 = ctypes.CDLL(ctypes.util.find_library("X11"))
xtst = ctypes.CDLL(ctypes.util.find_library("Xtst"))
x11.XOpenDisplay.restype = ctypes.c_void_p
x11.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
x11.XDefaultRootWindow.restype = ctypes.c_ulong
x11.XQueryTree.argtypes = [
    ctypes.c_void_p,
    ctypes.c_ulong,
    ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.c_ulong),
    ctypes.POINTER(ctypes.POINTER(ctypes.c_ulong)),
    ctypes.POINTER(ctypes.c_uint),
]
x11.XFetchName.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.POINTER(ctypes.c_char_p)]
x11.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x11.XKeysymToKeycode.restype = ctypes.c_uint
x11.XFree.argtypes = [ctypes.c_void_p]
x11.XFlush.argtypes = [ctypes.c_void_p]
x11.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
x11.XCloseDisplay.argtypes = [ctypes.c_void_p]
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
xtst.XTestFakeKeyEvent.argtypes = [
    ctypes.c_void_p,
    ctypes.c_uint,
    ctypes.c_int,
    ctypes.c_ulong,
]
xtst.XTestFakeKeyEvent.restype = ctypes.c_int
xtst.XTestFakeButtonEvent.argtypes = [
    ctypes.c_void_p,
    ctypes.c_uint,
    ctypes.c_int,
    ctypes.c_ulong,
]

display = x11.XOpenDisplay(None)
if not display:
    sys.exit("could not open X11 display")


def find_loading_bay(window: int) -> int | None:
    name = ctypes.c_char_p()
    if x11.XFetchName(display, window, ctypes.byref(name)) and name.value:
        if b"Loading Bay" in name.value:
            return window
    root = ctypes.c_ulong()
    parent = ctypes.c_ulong()
    children = ctypes.POINTER(ctypes.c_ulong)()
    count = ctypes.c_uint()
    if not x11.XQueryTree(
        display,
        window,
        ctypes.byref(root),
        ctypes.byref(parent),
        ctypes.byref(children),
        ctypes.byref(count),
    ):
        return None
    try:
        for index in range(count.value):
            found = find_loading_bay(children[index])
            if found is not None:
                return found
    finally:
        if children:
            x11.XFree(children)
    return None


root_window = x11.XDefaultRootWindow(display)
product_window = find_loading_bay(root_window)
if product_window is None:
    sys.exit("Loading Bay X11 window was not found")
x11.XWarpPointer(display, 0, product_window, 0, 0, 0, 0, 400, 300)
x11.XSync(display, False)
xtst.XTestFakeButtonEvent(display, 1, True, 0)
x11.XSync(display, False)
time.sleep(0.3)
xtst.XTestFakeButtonEvent(display, 1, False, 0)
x11.XSync(display, False)
for keysym in (0xFF1B, 0xFF0D):  # Escape, Return
    keycode = x11.XKeysymToKeycode(display, keysym)
    if keycode == 0:
        sys.exit(f"X11 keycode was unavailable for {keysym:#x}")
    xtst.XTestFakeKeyEvent(display, keycode, True, 0)
    x11.XSync(display, False)
    time.sleep(0.2)
    xtst.XTestFakeKeyEvent(display, keycode, False, 0)
    x11.XSync(display, False)
    time.sleep(0.2)

x11.XCloseDisplay(display)
