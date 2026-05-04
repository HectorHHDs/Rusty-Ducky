#!/usr/bin/env python3
"""
key_listener.py — ducky-rs host-side key bridge

Listens for keyboard events and sends key strings to the Pico's CDC data
port so DuckyScript WAIT_FOR_KEY can receive them.

Usage:
    python3 key_listener.py [--port /dev/ttyACM1] [--baud 115200]

Protocol:
    Sends newline-terminated key strings over CDC data port (ttyACM1).
    e.g. "a\n", "SHIFT+i\n", "CTRL+c\n", "ENTER\n", "F5\n"

    Key format:
      - Single char:          "a", "B", "1", "!"
      - With modifiers:       "CTRL+c", "SHIFT+F1", "ALT+TAB", "CTRL+SHIFT+ESC"
      - Special keys:         "ENTER", "TAB", "BACKSPACE", "ESC", "SPACE"
      - Function keys:        "F1" .. "F12"
      - Arrow keys:           "UP", "DOWN", "LEFT", "RIGHT"
      - Navigation:           "HOME", "END", "PAGEUP", "PAGEDOWN", "DELETE", "INSERT"

Dependencies:
    pip install pynput pyserial

Exit:
    Press Ctrl+C in the terminal running this script to stop.
"""

import argparse
import sys
import threading
import serial
from pynput import keyboard

# ---------------------------------------------------------------------------
# Key name mapping
# ---------------------------------------------------------------------------

SPECIAL_KEYS = {
    keyboard.Key.enter:     "ENTER",
    keyboard.Key.tab:       "TAB",
    keyboard.Key.backspace: "BACKSPACE",
    keyboard.Key.esc:       "ESC",
    keyboard.Key.space:     "SPACE",
    keyboard.Key.delete:    "DELETE",
    keyboard.Key.insert:    "INSERT",
    keyboard.Key.home:      "HOME",
    keyboard.Key.end:       "END",
    keyboard.Key.page_up:   "PAGEUP",
    keyboard.Key.page_down: "PAGEDOWN",
    keyboard.Key.up:        "UP",
    keyboard.Key.down:      "DOWN",
    keyboard.Key.left:      "LEFT",
    keyboard.Key.right:     "RIGHT",
    keyboard.Key.caps_lock: "CAPSLOCK",
    keyboard.Key.num_lock:  "NUMLOCK",
    keyboard.Key.scroll_lock: "SCROLLLOCK",
    keyboard.Key.print_screen: "PRINTSCREEN",
    keyboard.Key.pause:     "PAUSE",
    keyboard.Key.f1:  "F1",  keyboard.Key.f2:  "F2",  keyboard.Key.f3:  "F3",
    keyboard.Key.f4:  "F4",  keyboard.Key.f5:  "F5",  keyboard.Key.f6:  "F6",
    keyboard.Key.f7:  "F7",  keyboard.Key.f8:  "F8",  keyboard.Key.f9:  "F9",
    keyboard.Key.f10: "F10", keyboard.Key.f11: "F11", keyboard.Key.f12: "F12",
    keyboard.Key.media_play_pause: "MEDIA_PLAY_PAUSE",
    keyboard.Key.media_volume_up:  "MEDIA_VOLUME_UP",
    keyboard.Key.media_volume_down: "MEDIA_VOLUME_DOWN",
    keyboard.Key.media_volume_mute: "MEDIA_MUTE",
    keyboard.Key.media_next:       "MEDIA_NEXT",
    keyboard.Key.media_previous:   "MEDIA_PREV",
}

# Modifier keys we track for combo building
MODIFIERS = {
    keyboard.Key.ctrl_l, keyboard.Key.ctrl_r,
    keyboard.Key.shift, keyboard.Key.shift_l, keyboard.Key.shift_r,
    keyboard.Key.alt_l, keyboard.Key.alt_r, keyboard.Key.alt_gr,
    keyboard.Key.cmd, keyboard.Key.cmd_l, keyboard.Key.cmd_r,
}

# Current modifier state
held_mods = set()
port_lock  = threading.Lock()
ser        = None

# ---------------------------------------------------------------------------
# Key event handlers
# ---------------------------------------------------------------------------

def mod_prefix():
    """Build modifier prefix string like 'CTRL+SHIFT+'"""
    parts = []
    if keyboard.Key.ctrl_l  in held_mods or keyboard.Key.ctrl_r  in held_mods: parts.append("CTRL")
    if keyboard.Key.shift   in held_mods or keyboard.Key.shift_l in held_mods  \
                                         or keyboard.Key.shift_r in held_mods:  parts.append("SHIFT")
    if keyboard.Key.alt_l   in held_mods or keyboard.Key.alt_r   in held_mods  \
                                         or keyboard.Key.alt_gr  in held_mods:  parts.append("ALT")
    if keyboard.Key.cmd     in held_mods or keyboard.Key.cmd_l   in held_mods  \
                                         or keyboard.Key.cmd_r   in held_mods:  parts.append("GUI")
    return "+".join(parts) + "+" if parts else ""

def send_key(key_str: str):
    global ser
    if not key_str:
        return
    msg = (key_str + "\n").encode()
    with port_lock:
        if ser and ser.is_open:
            try:
                ser.write(msg)
                ser.flush()
                print(f"  → {key_str!r}")
            except serial.SerialException as e:
                print(f"[serial error] {e}", file=sys.stderr)

def on_press(key):
    if key in MODIFIERS:
        held_mods.add(key)
        return

    prefix = mod_prefix()

    if key in SPECIAL_KEYS:
        send_key(prefix + SPECIAL_KEYS[key])
    elif hasattr(key, 'char') and key.char is not None:
        ch = key.char
        # If SHIFT is held and char is uppercase letter, strip SHIFT from prefix
        # since the char already encodes the shift
        if "SHIFT+" in prefix and ch.isupper():
            prefix = prefix.replace("SHIFT+", "")
        key_str = prefix + ch if prefix else ch
        send_key(key_str)

def on_release(key):
    held_mods.discard(key)

# ---------------------------------------------------------------------------
# Serial connection with auto-reconnect
# ---------------------------------------------------------------------------

def connect_serial(port: str, baud: int) -> serial.Serial:
    while True:
        try:
            s = serial.Serial(port, baud, timeout=0.1)
            print(f"[key_listener] connected to {port} at {baud} baud")
            return s
        except serial.SerialException as e:
            print(f"[key_listener] waiting for {port}... ({e})", file=sys.stderr)
            import time; time.sleep(1)

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    global ser

    parser = argparse.ArgumentParser(description="ducky-rs key listener")
    parser.add_argument("--port", default="/dev/ttyACM1",
                        help="CDC data port (default: /dev/ttyACM1)")
    parser.add_argument("--baud", type=int, default=115200,
                        help="Baud rate (default: 115200)")
    args = parser.parse_args()

    print(f"[key_listener] starting — sending keys to {args.port}")
    print(f"[key_listener] press Ctrl+C here to stop\n")

    ser = connect_serial(args.port, args.baud)

    # Start keyboard listener in background thread
    listener = keyboard.Listener(on_press=on_press, on_release=on_release)
    listener.start()

    try:
        # Keep alive + handle serial reconnect
        import time
        while True:
            time.sleep(0.5)
            with port_lock:
                if not ser.is_open:
                    print("[key_listener] port closed, reconnecting...")
                    ser = connect_serial(args.port, args.baud)
    except KeyboardInterrupt:
        print("\n[key_listener] stopped")
    finally:
        listener.stop()
        with port_lock:
            if ser and ser.is_open:
                ser.close()

if __name__ == "__main__":
    main()
