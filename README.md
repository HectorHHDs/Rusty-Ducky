# ducky-rs

Rubber Ducky firmware for Raspberry Pi Pico, written in Rust with ~~Embassy~~ My own usb descriptor module WITH Embassy-usb

*The following is documentation on the tested Raspberry Pi Pico 1 build, the Pico 2 build is slightly different but not in any big ways.*

Implements the full DuckyScript 3.0 interpreter from the original CircuitPython
`pico-ducky` project, plus exclusive features not possible in CircuitPython:

- **ATTACKMODE HID+STORAGE** — when an SD card is soldered using the guide down below, the Pico
  presents itself as both a USB keyboard *and* a removable USB drive simultaneously if the payload runs ATTACKMODE HID STORAGE or ATTACKMODE HID CDC STORAGE
- **Serial payload manager** — upload/edit/run/delete `.dd` files over serial. Use PuTTY to access it on windows, and screen /dev/ttyACM* on Debian-based/ubuntu-based linux distros;
  without reflashing, on any Pico or Pico 2 (no WiFi required)
- **LED exfil monitor** — covert data channel via host lock-key LED toggles

---

## Hardware

| Pin  | Function          | Notes                                                                 |
|------|-------------------|-----------------------------------------------------------------------|
| GP22 | Button 1          | Triggers payload re-run                                               |
| GP4  | Payload 1 select  | Short to GND = payload.dd                                             |
| GP5  | Payload 2 select  | Short to GND = payload2.dd                                            |
| GP10 | Payload 3 select  | Short to GND = payload3.dd                                            |
| GP11 | Payload 4 select  | Short to GND = payload4.dd                                            |
| GP0  | Programming mode  | Short to GND = don't run payload, defaults to ATTACKMODE TERMINAL     |


## How to install an SD card onto your pico:

| SD Card Module | Pico GPIO   | Pico Physical Pin  |
|----------------|-------------|--------------------|
| VCC (3.3V)  -> |3V3          |Pin 36              |
| GND         -> |GND          |Pin 38 (or any GND) |
| MISO (DO)   -> |GP16         |Pin 21              |
| CS  (SS)    -> |GP17         |Pin 22              |
| SCK (CLK)   -> |GP18         |Pin 24              |
| MOSI (DI)   -> |GP19         |Pin 25              |

**SD card is fully optional.**

A few notes:
Use 3.3V not 5V. The Pico runs at 3.3V and so do most SD card modules. If you're using a bare SD card breakout (not a module), SD cards technically run on 3.3V natively so no level shifting is needed. If you're using a 5V Arduino-style SD module, it has its own regulator and level shifter on board — power it from VSYS (Pin 39, ~5V from USB) instead of 3V3, but still connect the signal lines directly to the GP pins.
A 10k pull-up resistor on CS (GP17) is recommended to keep the line high during boot before the firmware takes control, preventing the SD card from responding to garbage on the SPI bus during startup.
SPI0 is also used by other peripherals if you add them later. GP18/19/16 are the hardware SPI0 pins, which gives you the best throughput. If you ever need SPI0 for something else, you can remap the SD to SPI1 (GP10/11/12/13) but you'd need to update main.rs.
Card orientation — most breakout modules have the SD slot, decoupling capacitors, and voltage regulation already handled. The ones sold as "Micro SD Card Module" for Arduino work perfectly and cost about $1. Just make sure it's a 3.3V compatible module.

---

## One-time toolchain setup

```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add the Cortex-M0+ target (RP2040)
rustup target add thumbv6m-none-eabi

# Install the linker wrapper (puts stack at bottom of RAM for safety)
cargo install flip-link

# Install UF2 converter
cargo install elf2uf2-rs

# (Optional but recommended) install probe-rs for one-command flashing
cargo install probe-rs-tools
```

---

## Build

```bash
cd Little-Ducky

# Development build (faster compile, more debug info)
cargo build

# Release build (optimised for size — ~150 KB binary)
cargo build --release
```

---

## Flash

### Method 1 — UF2 drag-and-drop (no extra hardware)

```bash
# 1. Convert ELF to UF2
elf2uf2-rs target/thumbv6m-none-eabi/release/ducky little-ducky.uf2

# 2. Hold BOOTSEL button on the Pico while plugging in USB
#    The Pico appears as a drive called RPI-RP2

# 3. Drag little-ducky.uf2 onto the RPI-RP2 drive
#    The Pico reboots automatically and runs your firmware
```

### Method 2 — probe-rs (requires a debug probe or second Pico as SWD)

```bash
# Flash and immediately start watching defmt log output
cargo run --release
```

---

## First boot

On first boot, ducky-rs:

1. Probes SPI0 for an SD card — sets AttackMode based on payload - can be overridden to HID TERMINAL if GP0 pin is grounded, for development.
2. Formats the internal LittleFS region if not already formatted
3. Mounts the flash filesystem
4. Waits 500ms for USB enumeration
5. Checks GP0 — if pulled low, enters programming mode (no payload is activated, defaulted to ATTACKMODE HID TERMINAL)
6. Reads GP4/5/10/11 — selects `payload.dd` through `payload4.dd`, by default it runs payload.dd
7. Runs the selected payload (falls back to a built-in smoke test if no file)

The LED breathes slowly during idle and holds solid during exfil mode.

---

## Managing payloads

### Via serial terminal (any Pico)

Connect a serial terminal to the first USB CDC port (115200 baud):

```
# macOS / Linux
screen /dev/tty.usbmodem* 115200
# or
picocom -b 115200 /dev/ttyACM0

# Windows
PuTTY → Serial → COM? → 115200
```

You'll see the prompt:

```
ducky-rs payload manager
Type help for commands.
>
```

**Upload a payload:**
```
> put payload.dd
Send content. Type END on its own line to finish.
DELAY 500
GUI r
DELAY 500
STRINGLN notepad
ENTER
END
Wrote 52 bytes to payload.dd
>
```

**List files:**
```
> list
  payload.dd
  payload2.dd
>
```

**Run a payload immediately (non-blocking):**
```
> run payload.dd
Queued: payload.dd
>
```

**Read a file back:**
```
> get payload.dd
DELAY 500
GUI r
...
>
```

**Hex dump loot.bin:**
```
> exfil
loot.bin: 8 bytes
00000000: 48 65 6C 6C 6F 21 0A 00  |Hello!..|
>
```

**Soft reset:**
```
> reboot
Rebooting...
```

### Via USB drive (SD card required)

If an SD card is present, the Pico appears as a removable USB drive on the host.
Drop `.dd` files directly onto it. On the next button press (or reboot), ducky-rs
reads the payload from the SD card (SD takes priority over flash).

---

## Writing payloads

See `COMMAND_REFERENCE.md` for the full DuckyScript 3.0 + extensions reference.

### Quick example — open terminal and run a command (Windows)

```duckyscript
REM Open Run dialog, launch PowerShell hidden, run command
DELAY 500
GUI r
DELAY 600
STRING powershell -w hidden -c "whoami > C:\loot.txt"
ENTER
```

### Using WAIT_FOR_BUTTON with timeout

```duckyscript
REM Short press = payload A, long press = payload B, no press = default
WAIT_FOR_BUTTON button1 3000 $pressed
IF ($pressed == FALSE)
    IMPORT default.dd
ELSE IF ($_BUTTON_ELAPSED_MS > 1000)
    IMPORT payload_b.dd
ELSE
    IMPORT payload_a.dd
END_IF
```

### LED-based exfil (advanced)

```duckyscript
REM Start LED exfil mode — host must run the matching receiver script
$_EXFIL_MODE_ENABLED = TRUE
REM ... exfil happens in background via lock key toggles ...
REM ScrollLock toggle stops collection
```

### WAIT_FOR_KEY (requires key_listener.py on host)

```duckyscript
REM Wait for user to confirm before continuing
PRINT Ready? Press any key in your terminal...
ENTER
VAR $k = ""
WAIT_FOR_KEY $k
PRINT Received: $k
ENTER
```

Start key_listener.py on the host:
```bash
python key_listener.py            # auto-detects Pico serial port
python key_listener.py COM5       # specify port manually (Windows)
python key_listener.py /dev/ttyACM1  # specify port (Linux)
```

---

## Layout switching

```duckyscript
REM Switch to German layout for typing umlauts
DUCKY_LANG DE
STRING Straße
ENTER

REM Switch to French AZERTY
DUCKY_LANG FR
STRING Bonjour

REM Back to US
DUCKY_LANG US
```

Implemented layouts: **US**, **DE**, **FR**
Stubbed (fall back to US): ES, IT, PT, UK

---

## Feature flags at boot

| Condition                     | USB descriptor     | SD accessible |
|-------------------------------|--------------------|---------------|
| No SD card soldered           | HID only           | —             |
| SD present, GP15 floating     | HID + MSC          | Yes (USB drive) |
| SD present, GP15 to GND       | HID only           | Yes (script only) |
| GP0 to GND                    | HID only (no payload) | Yes        |

---

## Flash memory layout

```
0x10000000  Boot2 (256 B)      — RP2040 second-stage bootloader
0x10000100  Firmware           — ~150 KB (ducky-rs binary)
0x10040000  LittleFS           — 1792 KB payload storage
0x101FFFFF  End of 2MB flash
```

LittleFS is auto-formatted on first boot. Payload files (`.dd`) and `loot.bin`
live here. The flash filesystem is **separate** from the USB drive — the USB
drive is backed by the SD card, not the internal flash.

---

## Comparison with CircuitPython version

| Feature                  | CircuitPython         | ducky-rs (Rust)        |
|--------------------------|-----------------------|------------------------|
| DuckyScript 3.0          | ✅ Full               | ✅ Full                |
| HOLD / RELEASE           | ✅                    | ✅                     |
| DEFINE macros            | ✅                    | ✅                     |
| Runtime layout switch    | ✅ (dynamic import)   | ✅ (compiled-in tables)|
| WAIT_FOR_BUTTON timeout  | ❌                    | ✅ New                 |
| $_BUTTON_ELAPSED_MS      | ❌                    | ✅ New                 |
| USB HID + MSC combined   | ❌                    | ✅ (SD required)       |
| Payload manager          | WiFi webapp (Pico W)  | Serial terminal (any)  |
| Flash filesystem         | CircuitPython VFS     | LittleFS               |
| Startup time             | ~3s (Python boot)     | <100ms                 |
| Binary size              | ~600KB runtime        | ~150KB total           |
