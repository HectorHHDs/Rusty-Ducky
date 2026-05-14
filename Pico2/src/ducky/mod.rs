//! ducky/mod.rs — payload runner and filesystem integration

pub mod executor;
pub mod validator;
pub mod expr;
pub mod keys;
pub mod parser;

use defmt::*;
use heapless::Vec;

use executor::ScriptContext;
// ---------------------------------------------------------------------------
// Flash filesystem helpers — all async, go through flash_task channel
// ---------------------------------------------------------------------------

pub fn init_flash_fs() {
    info!("[ducky] flash storage via flash_task");
}





// ---------------------------------------------------------------------------
// Run a payload
// ---------------------------------------------------------------------------

// Static script buffer — avoids 256KB stack allocation when running payloads
// 64KB is enough for practical DuckyScript payloads
static mut SCRIPT_BUF: [u8; 65536] = [0u8; 65536];  // 64KB
static mut SCRIPT_BUF_LEN: usize = 0;

pub async fn run_payload(filename: &str) {
    info!("[ducky] run_payload: {}", filename);

    let fs = crate::fs::FlashFs::new();
    if !fs.file_exists_async(filename).await {
        info!("[ducky] {} not found — skipping", filename);
        return;
    }
    let len = match fs.read_file_async(filename).await {
        Ok(n)  => n,
        Err(e) => { warn!("[ducky] read error: {}", e); return; }
    };
    let len2 = len.min(crate::fs::DATA_BUF_SIZE);
    unsafe {
        SCRIPT_BUF[..len2].copy_from_slice(&crate::fs::FLASH_DATA_BUF[..len2]);
        SCRIPT_BUF_LEN = len2;
    }

    let script = match core::str::from_utf8(unsafe { &SCRIPT_BUF[..SCRIPT_BUF_LEN] }) {
        Ok(s)  => s,
        Err(_) => { warn!("[ducky] {} not valid UTF-8", filename); return; }
    };

    run_script_text(script).await;
}

// ScriptContext is large — keep it static to avoid bloating the future size
static mut SCRIPT_CTX: Option<ScriptContext> = None;

pub async fn run_script_text(script: &str) {
    let raw: Vec<&str, 64> = script.lines().collect();
    // Initialise context in static storage
    unsafe { SCRIPT_CTX = Some(ScriptContext::new()); }
    let ctx = unsafe { SCRIPT_CTX.as_mut().unwrap() };
    let after_defines = executor::collect_defines(&raw, ctx);
    let exec_lines    = executor::collect_functions(&after_defines, ctx);
    let exec_refs: Vec<&str, 64> = exec_lines.iter().map(|s| s.as_str()).collect();

    loop {
        match ctx.execute(&exec_refs).await {
            executor::ExecResult::Restart  => {
                info!("[ducky] RESTART_PAYLOAD");
                unsafe { SCRIPT_CTX = Some(ScriptContext::new()); }
                let ctx2 = unsafe { SCRIPT_CTX.as_mut().unwrap() };
                let ad2 = executor::collect_defines(&raw, ctx2);
                let el2 = executor::collect_functions(&ad2, ctx2);
                let er2: Vec<&str, 64> = el2.iter().map(|s| s.as_str()).collect();
                let _ = ctx2.execute(&er2).await;
                break;
            }
            executor::ExecResult::Stop     => { info!("[ducky] STOP_PAYLOAD"); break; }
            executor::ExecResult::Continue => break,
        }
    }
}

// ---------------------------------------------------------------------------
// Built-in test scripts
// ---------------------------------------------------------------------------

fn get_test_script(filename: &str) -> &'static str {
    match filename {
        "payload2.dd" => PAYLOAD2,
        "payload3.dd" => PAYLOAD3,
        "payload4.dd" => PAYLOAD4,
        _             => PAYLOAD1,
    }
}

const PAYLOAD1: &str = "\
REM ducky-rs smoke test
DELAY 500
LED
GUI r
DELAY 600
STRINGLN notepad
DELAY 800
STRING Hello from ducky-rs!
ENTER
CTRL s
DELAY 400
STRINGLN ducky_test.txt
DELAY 400
ALT F4
LED
";

const PAYLOAD2: &str = "\
REM Mouse test
DELAY 500
MOUSE MOVE 100,0
DELAY 100
MOUSE CLICK LEFT
";

const PAYLOAD3: &str = "\
REM Control flow test
DEFINE #REPS 3
VAR $i = 0
WHILE ($i < #REPS)
    $i = ($i + 1)
END_WHILE
GUI r
DELAY 500
STRINGLN notepad
DELAY 800
STRING Loop ran #REPS times
ENTER
ALT F4
";

const PAYLOAD4: &str = "\
REM WAIT_FOR_KEY test
DELAY 500
GUI r
DELAY 500
STRINGLN notepad
DELAY 800
VAR $k = \"\"
WAIT_FOR_KEY $k
STRING Got: $k
ENTER
ALT F4
";

