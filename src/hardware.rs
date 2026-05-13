//! hardware.rs — GPIO tasks: LED breathing, debounced button monitor

use embassy_rp::gpio::{Input, Output};
use embassy_time::{Duration, Instant, Timer};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal, channel::Channel};
use defmt::*;

// ---------------------------------------------------------------------------
// Inter-task signals
// ---------------------------------------------------------------------------

pub static SCRIPT_SIGNAL: Channel<CriticalSectionRawMutex, &'static str, 4> = Channel::new();
pub static WAIT_BUTTON_SIGNAL: Signal<CriticalSectionRawMutex, u64>          = Signal::new();
pub static EXFIL_LEDS_ENABLED: Signal<CriticalSectionRawMutex, bool>         = Signal::new();

// ---------------------------------------------------------------------------
// Payload selector
// ---------------------------------------------------------------------------

pub fn select_payload(p1: &Input, p2: &Input, p3: &Input, p4: &Input) -> &'static str {
    if p1.is_low()      { "payload.dd"  }
    else if p2.is_low() { "payload2.dd" }
    else if p3.is_low() { "payload3.dd" }
    else if p4.is_low() { "payload3.dd" }  // p4 now maps to payload3.dd (payload4 removed)
    else                { "payload.dd"  }
}

// ---------------------------------------------------------------------------
// LED task — software PWM breathing
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn led_task(mut led: Output<'static>) {
    info!("led_task started");
    const LEVELS: usize = 32;
    const STEP_MS: u64  = 15;
    let mut exfil_on = false;

    loop {
        if EXFIL_LEDS_ENABLED.signaled() {
            exfil_on = EXFIL_LEDS_ENABLED.wait().await;
        }
        if exfil_on {
            led.set_high();
            Timer::after(Duration::from_millis(50)).await;
            continue;
        }
        for i in 0..LEVELS {
            let on_ms  = ((i as u64 + 1) * STEP_MS) / LEVELS as u64;
            let off_ms = STEP_MS - on_ms;
            if on_ms  > 0 { led.set_high(); Timer::after(Duration::from_millis(on_ms)).await; }
            if off_ms > 0 { led.set_low();  Timer::after(Duration::from_millis(off_ms)).await; }
        }
        for i in (0..LEVELS).rev() {
            let on_ms  = ((i as u64 + 1) * STEP_MS) / LEVELS as u64;
            let off_ms = STEP_MS - on_ms;
            if on_ms  > 0 { led.set_high(); Timer::after(Duration::from_millis(on_ms)).await; }
            if off_ms > 0 { led.set_low();  Timer::after(Duration::from_millis(off_ms)).await; }
        }
    }
}

// ---------------------------------------------------------------------------
// Button task
// ---------------------------------------------------------------------------

#[embassy_executor::task]
pub async fn button_task(
    mut button: Input<'static>,
    p1: Input<'static>,
    p2: Input<'static>,
    p3: Input<'static>,
    p4: Input<'static>,
) {
    info!("button_task started (GP22)");
    const DEBOUNCE_MS: u64 = 20;

    loop {
        button.wait_for_falling_edge().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;
        if button.is_high() { continue; }

        info!("Button pressed");
        let press_time = Instant::now();

        button.wait_for_rising_edge().await;
        Timer::after(Duration::from_millis(DEBOUNCE_MS)).await;

        let elapsed_ms = press_time.elapsed().as_millis();
        info!("Button released (held {}ms)", elapsed_ms);

        if WAIT_BUTTON_SIGNAL.signaled() {
            WAIT_BUTTON_SIGNAL.signal(elapsed_ms);
        } else {
            let payload = select_payload(&p1, &p2, &p3, &p4);
            info!("Triggering payload: {}", payload);
            SCRIPT_SIGNAL.try_send(payload).ok();
        }
    }
}

// ---------------------------------------------------------------------------
// WAIT_FOR_BUTTON helper (called from parser.rs)
// ---------------------------------------------------------------------------

pub async fn wait_for_button(timeout_ms: Option<u64>) -> (bool, u64) {
    match timeout_ms {
        None => {
            let elapsed = WAIT_BUTTON_SIGNAL.wait().await;
            (true, elapsed)
        }
        Some(ms) => {
            use embassy_time::with_timeout;
            match with_timeout(Duration::from_millis(ms), WAIT_BUTTON_SIGNAL.wait()).await {
                Ok(elapsed) => (true, elapsed),
                Err(_)      => { info!("[WAIT_FOR_BUTTON] Timeout after {}ms", ms); (false, ms) }
            }
        }
    }
}
