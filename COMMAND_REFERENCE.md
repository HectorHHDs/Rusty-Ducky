# DuckyScript Command Reference
> Based on `duckyinpython.py` — DuckyScript 3.0 with custom extensions

Indentation is supported everywhere. Lines starting with `REM` are comments.

---

## REM
**Comment. Line is ignored entirely.**

```
REM This is a comment
REM TODO: add delay here
```

---

## DELAY
**Pause execution for a number of milliseconds.**

```
DELAY <ms>
```

```
DELAY 500        REM wait 500ms
DELAY 1000       REM wait 1 second
DELAY 50         REM short pause between keystrokes
```

---

## DEFAULT_DELAY / DEFAULTDELAY
**Set a delay (ms) automatically inserted after every command.**
Both spellings work identically.

```
DEFAULT_DELAY <ms>
DEFAULTDELAY <ms>
```

```
DEFAULT_DELAY 100    REM add 100ms after every command
STRING hello         REM types "hello", then waits 100ms
ENTER                REM presses Enter, then waits 100ms
DEFAULT_DELAY 0      REM reset to no auto-delay
```

---

## STRING
**Type a string of characters on the host machine.**
Variable interpolation is supported — `$VAR` tokens are expanded before typing.

```
STRING <text>
```

```
STRING Hello, World!
STRING The answer is $answer
STRING C:\Users\$username\Documents

REM variables are expanded:
VAR $name = "Alice"
STRING My name is $name
REM types: My name is Alice
```

---

## Key combos (bare keynames)
**Press and release one or more keys simultaneously.**
Any key name from the table below, space-separated.

```
ENTER
GUI r
CTRL ALT DELETE
SHIFT F10
ALT F4
CTRL c
```

Available key names:
`WINDOWS` `GUI` `COMMAND` `SHIFT` `RSHIFT` `ALT` `RALT` `CONTROL` `CTRL` `RCTRL`
`APP` `MENU` `TAB` `ENTER` `SPACE` `BACKSPACE` `DELETE` `INSERT` `HOME` `END`
`PAGEUP` `PAGEDOWN` `UP` `DOWN` `LEFT` `RIGHT` `CAPSLOCK` `NUMLOCK` `SCROLLLOCK`
`PRINTSCREEN` `PAUSE` `ESC` `ESCAPE`
`F1`–`F24`
`A`–`Z`

```
REM Open Run dialog and launch notepad
GUI r
DELAY 500
STRING notepad
ENTER

REM Select all and copy
CTRL a
DELAY 100
CTRL c
```

---

## REPEAT
**Repeat the immediately previous command N times.**

```
REPEAT <count>
```

```
STRING ha
REPEAT 5       REM types "ha" 5 more times → hahahahahaha

BACKSPACE
REPEAT 9       REM press backspace 10 times total
```

---

## PRINT
**Print a message to the debug log (serial console).**
Variable interpolation supported.

```
PRINT <message>
```

```
PRINT Starting payload
PRINT Attempt $attempts of $max
PRINT Done!

VAR $x = 42
PRINT The value is $x
REM outputs: [SCRIPT]: The value is 42
```

---

## VAR
**Declare and initialise a variable.**
Variables must start with `$`. Values can be integers, booleans, strings, or expressions.

```
VAR $name = <value or expression>
```

```
VAR $count   = 0
VAR $max     = 10
VAR $flag    = FALSE
VAR $message = "hello"
VAR $rolled  = RANDOM_INT(1, 6)
VAR $double  = ($count * 2)
```

---

## Variable assignment (bare)
**Reassign an existing variable without `VAR`.**

```
$name = <value or expression>
```

```
VAR $x = 0
$x = ($x + 1)
$x = ($x * 2)
$flag = TRUE
$message = "done"
```

---

## RANDOMIZE
**Re-seed the random number generator from the current time.**
Call before using `RANDOM_INT` if you want different results each run.

```
RANDOMIZE
```

```
RANDOMIZE
VAR $delay = RANDOM_INT(200, 800)
DELAY $delay
```

---

## RANDOM_INT
**Generate a random integer between min and max (inclusive).**
Used inside expressions and VAR assignments, not as a standalone command.

```
RANDOM_INT(<min>, <max>)
```

```
VAR $n = RANDOM_INT(1, 100)
PRINT Random number: $n

VAR $wait = RANDOM_INT(500, 2000)
DELAY $wait

REM also works directly in conditions:
WHILE (RANDOM_INT(1,2) == 1)
    PRINT Heads
END_WHILE
```

---

## IF / ELSE IF / ELSE / END_IF
**Conditional branching.**
Conditions support: `==` `!=` `<` `<=` `>` `>=` `&&` `||` `!`
Optional `THEN` keyword allowed.

```
IF <condition>
    ...
ELSE IF <condition>
    ...
ELSE
    ...
END_IF
```

```
VAR $x = 5

IF ($x > 10)
    PRINT big
ELSE IF ($x > 3)
    PRINT medium
ELSE
    PRINT small
END_IF

REM with THEN keyword (optional):
IF ($x == 5) THEN
    PRINT exactly five
END_IF

REM boolean variable:
VAR $ready = TRUE
IF ($ready == TRUE)
    STRING ready!
    ENTER
END_IF

REM negation:
VAR $done = FALSE
IF (!$done)
    PRINT not done yet
END_IF
```

---

## WHILE / END_WHILE
**Loop while a condition is true.**

```
WHILE <condition>
    ...
END_WHILE
```

```
VAR $i = 0
WHILE ($i < 5)
    PRINT i is $i
    $i = ($i + 1)
END_WHILE

REM infinite loop until flag is set:
VAR $stop = FALSE
WHILE ($stop == FALSE)
    WAIT_FOR_KEY $k
    IF ($k == "q")
        $stop = TRUE
    END_IF
END_WHILE
```

---

## FOR / END_FOR
**Loop a variable from a start to an end value, with optional step.**
Both start and end are inclusive.

```
FOR $var FROM <start> TO <end>
FOR $var FROM <start> TO <end> STEP <step>
    ...
END_FOR
```

```
REM count up: 1 2 3 4 5
FOR $i FROM 1 TO 5
    PRINT $i
END_FOR

REM count down: 10 8 6 4 2
FOR $i FROM 10 TO 2 STEP -2
    PRINT $i
END_FOR

REM use variable as bound:
VAR $max = RANDOM_INT(3, 7)
FOR $i FROM 1 TO $max
    STRING echo $i
    ENTER
END_FOR
```

---

## FUNCTION / END_FUNCTION
**Define a reusable block of commands.**
Functions are collected before execution — they can be defined anywhere in the file.
Call them by name followed by `()`.

```
FUNCTION name()
    ...
END_FUNCTION

name()
```

```
FUNCTION open_run()
    GUI r
    DELAY 500
END_FUNCTION

FUNCTION type_and_enter()
    STRING cmd /c whoami
    ENTER
    DELAY 800
END_FUNCTION

open_run()
type_and_enter()

REM functions can use variables:
VAR $target = "notepad.exe"
FUNCTION launch()
    GUI r
    DELAY 500
    STRING $target
    ENTER
END_FUNCTION

launch()
```

---

## RESTART_PAYLOAD
**Stop execution and restart the script from the beginning.**

```
RESTART_PAYLOAD
```

```
VAR $tries = 0
WHILE ($tries < 3)
    $tries = ($tries + 1)
    IF ($tries == 2)
        RESTART_PAYLOAD    REM starts over completely
    END_IF
END_WHILE
```

---

## STOP_PAYLOAD
**Stop execution immediately. Script does not restart.**

```
STOP_PAYLOAD
```

```
VAR $success = FALSE

REM ... do things ...

IF ($success == TRUE)
    PRINT Done!
    STOP_PAYLOAD
END_IF

PRINT Failed — should not reach here if success
```

---

## LED
**Toggle the onboard LED on/off.**

```
LED
```

```
LED          REM turn on
DELAY 200
LED          REM turn off
DELAY 200
LED          REM turn on again

REM blink 3 times:
FOR $i FROM 1 TO 3
    LED
    DELAY 150
    LED
    DELAY 150
END_FOR
```

---

## SAVE_HOST_KEYBOARD_STATE
**Save the current state of CapsLock, NumLock, and ScrollLock.**
Use before a payload that might change lock key states.

```
SAVE_HOST_KEYBOARD_STATE
```

---

## RESTORE_HOST_KEYBOARD_STATE
**Restore CapsLock, NumLock, and ScrollLock to their saved state.**
Call after your payload to leave the keyboard as you found it.

```
RESTORE_HOST_KEYBOARD_STATE
```

```
SAVE_HOST_KEYBOARD_STATE

CAPSLOCK               REM toggle caps (payload might need it)
STRING HELLO WORLD
ENTER
CAPSLOCK               REM toggle back (but state might be wrong)

RESTORE_HOST_KEYBOARD_STATE   REM guaranteed correct regardless
```

---

## MOUSE
**Control the mouse.**

```
MOUSE MOVE <x>,<y>         REM move relative by x,y pixels
MOUSE CLICK LEFT           REM left click
MOUSE CLICK RIGHT          REM right click
MOUSE PRESS LEFT           REM hold left button down
MOUSE RELEASE LEFT         REM release left button
MOUSE WHEEL <amount>       REM scroll (positive=up, negative=down)
```

```
MOUSE MOVE 100,0           REM move right 100px
MOUSE CLICK LEFT           REM click
MOUSE MOVE 0,50            REM move down 50px
MOUSE CLICK RIGHT          REM right click

MOUSE WHEEL 3              REM scroll up 3 units
MOUSE WHEEL -5             REM scroll down 5 units

REM drag:
MOUSE PRESS LEFT
MOUSE MOVE 200,100
MOUSE RELEASE LEFT
```

---

## CC
**Send a Consumer Control key (media keys, brightness, etc.).**

```
CC <CONSUMER_CONTROL_CODE>
```

```
CC MUTE
CC VOLUME_INCREMENT
CC VOLUME_DECREMENT
CC PLAY_PAUSE
CC SCAN_NEXT_TRACK
CC SCAN_PREVIOUS_TRACK
CC STOP
CC BRIGHTNESS_INCREMENT
CC BRIGHTNESS_DECREMENT
```

---

## WAIT_FOR_BUTTON
**Block until the Pico's physical button (GP22) is pressed and released.**

Optionally accepts a timeout in milliseconds. If the button is not pressed within
the timeout, execution continues on the next line (the variable is set to `FALSE`).
With no timeout (or timeout `0`) the command waits indefinitely — original behaviour.

After every call, `$_BUTTON_ELAPSED_MS` is set to the number of milliseconds the
button was held down (useful for detecting short vs long presses).

```
WAIT_FOR_BUTTON button1
WAIT_FOR_BUTTON button1 <timeout_ms>
WAIT_FOR_BUTTON button1 <timeout_ms> $VARIABLE
```

| Argument | Description |
|---|---|
| `button1` | The button to wait on (only `button1` / GP22 supported) |
| `timeout_ms` | Optional. `0` or omitted = wait forever. `>0` = give up after N ms. |
| `$VARIABLE` | Optional. Set to `TRUE` if pressed, `FALSE` if timeout elapsed. |

```
REM wait indefinitely (original behaviour):
PRINT Waiting for button...
WAIT_FOR_BUTTON button1
PRINT Button pressed, continuing.

REM with timeout — non-blocking if nobody presses:
VAR $pressed = FALSE
WAIT_FOR_BUTTON button1 3000 $pressed
IF ($pressed == TRUE)
    PRINT Got it within 3 seconds!
ELSE
    PRINT No press — moving on.
END_IF

REM detect short vs long press using $_BUTTON_ELAPSED_MS:
WAIT_FOR_BUTTON button1
IF ($_BUTTON_ELAPSED_MS > 1000)
    PRINT Long press detected
ELSE
    PRINT Short press detected
END_IF

REM use as a manual step trigger:
FOR $step FROM 1 TO 3
    PRINT Press button to run step $step
    WAIT_FOR_BUTTON button1
    STRING echo step $step
    ENTER
END_FOR

REM timed gate — run extra payload only if button pressed in 5s:
WAIT_FOR_BUTTON button1 5000 $shortcut
IF ($shortcut == TRUE)
    IMPORT extra_payload.dd
END_IF
```

---

## WAIT_FOR_KEY
**Block until the host sends a keypress via `key_listener.py` over CDC serial.**
Requires `key_listener.py` running on the host and CDC data enabled in `boot.py`.
Optionally saves the key string (e.g. `"SHIFT+I"`, `";"`) to a variable.

```
WAIT_FOR_KEY
WAIT_FOR_KEY $VARIABLE
WAIT_FOR_KEY $VARIABLE {wait time in ms for the keypress, before passing to next line}
```

Key string format examples: `a`, `SHIFT+I`, `CTRL+C`, `ENTER`, `F5`, `;`, `SPACE`

```
REM wait and discard:
WAIT_FOR_KEY

REM capture and print:
VAR $k = ""
WAIT_FOR_KEY $k
PRINT You pressed: $k

REM wait for a specific key:
VAR $k = ""
WHILE ($k != "ENTER")
    WAIT_FOR_KEY $k
    PRINT Got: $k
END_WHILE
PRINT Enter was pressed.

REM collect 5 keypresses:
VAR $i = 0
WHILE ($i < 5)
    WAIT_FOR_KEY $k
    PRINT Key $i: $k
    $i = ($i + 1)
END_WHILE
```

---

## Built-in variables

| Variable | Set by | Description |
|---|---|---|
| `$_INITIAL_CAPSLOCK` | `SAVE_HOST_KEYBOARD_STATE` | CapsLock state at save time |
| `$_INITIAL_NUMLOCK` | `SAVE_HOST_KEYBOARD_STATE` | NumLock state at save time |
| `$_INITIAL_SCROLLLOCK` | `SAVE_HOST_KEYBOARD_STATE` | ScrollLock state at save time |
| `$_EXFIL_MODE_ENABLED` | Script | Enables LED exfil mode |
| `$_EXFIL_LEDS_ENABLED` | Script | Holds LED on during exfil |

---

## Expression operators

| Operator | Meaning |
|---|---|
| `+` `-` `*` `/` `%` | Arithmetic |
| `==` `!=` `<` `<=` `>` `>=` | Comparison |
| `&&` | Logical AND |
| `\|\|` | Logical OR |
| `!` | Logical NOT |

```
VAR $a = 10
VAR $b = 3
VAR $c = ($a + $b)         REM 13
VAR $d = ($a * $b)         REM 30
VAR $e = ($a % $b)         REM 1
VAR $big = ($a > 5)        REM TRUE
VAR $both = ($a > 5 && $b < 10)   REM TRUE
VAR $either = ($a == 0 || $b == 3) REM TRUE
VAR $notbig = !($a > 5)    REM FALSE
```

---

## STRINGLN
**Type a string of characters then press Enter.**
Equivalent to `STRING <text>` followed by `ENTER`. Variable interpolation supported.

```
STRINGLN <text>
```

```
STRINGLN Hello, World!
REM types "Hello, World!" then presses Enter

STRINGLN whoami
REM types "whoami" then presses Enter — useful for shell commands

VAR $cmd = "ipconfig /all"
STRINGLN $cmd
REM types the variable contents then presses Enter

REM open Run, launch notepad in one line:
GUI r
DELAY 500
STRINGLN notepad
```

---

## STRING_DELAY
**Set a per-character delay (ms) for all subsequent STRING and STRINGLN commands.**
Set to `0` to restore the default adaptive delays.
Useful for slow targets or evading typing-speed detection heuristics.

```
STRING_DELAY <ms>
```

```
STRING_DELAY 100     REM 100ms between every character
STRING Hello         REM types slowly: H...e...l...l...o

STRING_DELAY 0       REM reset to default adaptive timing
STRING fast now      REM back to normal speed

REM slow typing looks more human:
STRING_DELAY RANDOM_INT(30, 120)
STRING This looks like a human typed it
```

---

## HOLD
**Press and hold a key without releasing it.**
The key stays down until `RELEASE <key>` or `RELEASE ALL` is called.
Multiple keys can be held simultaneously.

```
HOLD <key>
```

```
HOLD GUI
DELAY 200
HOLD SHIFT         REM both GUI and SHIFT are now held
DELAY 100
RELEASE ALL        REM release everything

REM hold Shift while typing for capitals without CAPS LOCK:
HOLD SHIFT
STRING hello       REM types HELLO
RELEASE SHIFT

REM hold a modifier across multiple keystrokes:
HOLD CTRL
STRING c           REM Ctrl+C
DELAY 100
STRING v           REM Ctrl+V
RELEASE CTRL
```

Available key names: same as the key combo table above (`GUI`, `SHIFT`, `ALT`, `CTRL`, `A`–`Z`, `F1`–`F24`, etc.)

---

## RELEASE
**Release a previously held key, or release all held keys at once.**
Safe to call even if the key was not explicitly held.

```
RELEASE <key>
RELEASE ALL
```

```
HOLD ALT
DELAY 200
RELEASE ALT        REM release just Alt

HOLD GUI
HOLD SHIFT
HOLD S             REM three keys held
RELEASE ALL        REM drop everything at once

REM defensive cleanup at end of payload:
RELEASE ALL
```

---

## DEFINE
**Define a compile-time text substitution macro.**
`DEFINE` lines are processed before execution — every occurrence of `#NAME` anywhere
in the script is replaced with its value before any command runs.
Macro names must start with `#`. Multiple macros are replaced longest-first to avoid
partial matches.

```
DEFINE #NAME <value>
```

```
DEFINE #DELAY    500
DEFINE #TARGET   notepad.exe
DEFINE #USER     Administrator

DELAY #DELAY
GUI r
DELAY #DELAY
STRING #TARGET
ENTER

REM numeric macros work in expressions:
DEFINE #MAX 10
FOR $i FROM 1 TO #MAX
    PRINT $i
END_FOR

REM boolean macros:
DEFINE #ENABLED TRUE
VAR $flag = #ENABLED

REM macros inside conditions:
DEFINE #THRESHOLD 5
IF ($x > #THRESHOLD)
    PRINT above threshold
END_IF

REM macros compose with variables:
DEFINE #CMD "powershell -w hidden"
STRING #CMD -c whoami
ENTER
```

---

## ATTACKMODE
**Sets the attackmode to HID, CDC, STORAGE, TERMINAL, or combinations of these.**
`ATTACKMODE` Can be switched during runtime, but it will reboot the pico whenever the ATTACKMODE is switched, so it is heavily advised to set it only once during the script.

```
ATTACKMODE HID
REM this attackmode blocks access to the terminal, and does not use storage nor CDC (CDC is required for WAIT_FOR_BUTTON and WAIT_FOR_KEY, along with the key_listener.py script running on host.
DELAY 500
STRING this is in HID mode! so you can't edit it until you connect GND to GP0, which defaults to ATTACKMODE HID CDC TERMINAL
```

---

## DUCKY_LANG
**Switch the active keyboard layout at runtime.**
Updates the layout used by all subsequent `STRING`, `STRINGLN`, `HOLD`, and key combo
commands. The matching `keyboard_layout_win_<LANG>.py` and `keycode_win_<LANG>.py`
files must be present in `/lib` on the Pico.
With no argument, prints the currently active layout to the debug log.

```
DUCKY_LANG <lang>
DUCKY_LANG
```

Supported layout codes:
`US` `DE` `FR` `ES` `IT` `PT` `UK` `BE` `CA` `DK` `FI` `NO` `SE` `CH` `PL` `RU` `TR` `JP` `KR` `BR`

```
REM switch to German layout before typing:
DUCKY_LANG DE
STRING Straße
ENTER

REM switch back to US:
DUCKY_LANG US

REM conditional layout based on $_HOST_OS:
IF ($os == "DE")
    DUCKY_LANG DE
ELSE IF ($os == "FR")
    DUCKY_LANG FR
ELSE
    DUCKY_LANG US
END_IF
STRING Hello
ENTER

REM print current layout to debug log:
DUCKY_LANG
```

> **Note:** Layout detection from the host is not possible over USB HID —
> the host never tells the Pico what layout it is using. You must set
> `DUCKY_LANG` manually based on prior knowledge of the target.

---

## Built-in system variables (extended)

The following variables are maintained automatically by the runtime and are available
to read in any expression or condition. They extend the original built-in variable table.

| Variable | Updated | Description |
|---|---|---|
| `$_TARGET_LOCK_KEYS` | Every line | Bitmask of active lock keys: `1`=NumLock, `2`=CapsLock, `4`=ScrollLock |
| `$_JITTER_ENABLED` | Script (read/write) | Set `TRUE` to add random extra delay after every command |
| `$_JITTER_MAX_DELAY` | Script (read/write) | Max extra ms added per command when jitter is on (default `50`) |
| `$_HOST_OS` | Script (read/write) | Set manually: `"WINDOWS"` / `"MACOS"` / `"LINUX"` / `"UNKNOWN"` |
| `$_ACTIVE_LAYOUT` | `DUCKY_LANG` | Current layout identifier, e.g. `"US"`, `"DE"` |
| `$_BUTTON_ELAPSED_MS` | `WAIT_FOR_BUTTON` | Ms the button was held during the last `WAIT_FOR_BUTTON` call |

```
REM check if CapsLock is on (bit 1 of bitmask):
IF (($_TARGET_LOCK_KEYS && 2) == 2)
    PRINT CapsLock is ON
END_IF

REM enable jitter for human-like timing:
$_JITTER_ENABLED   = TRUE
$_JITTER_MAX_DELAY = 80
STRING This typing has random variation
ENTER
$_JITTER_ENABLED   = FALSE

REM branch on OS set earlier by the payload:
$_HOST_OS = "WINDOWS"
IF ($_HOST_OS == "WINDOWS")
    GUI r
    DELAY 500
    STRINGLN cmd
ELSE IF ($_HOST_OS == "MACOS")
    GUI SPACE
    DELAY 500
    STRINGLN Terminal
END_IF

REM read the active layout:
PRINT Current layout: $_ACTIVE_LAYOUT
```
