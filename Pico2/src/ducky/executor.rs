//! ducky/executor.rs — DuckyScript interpreter
//!
//! Kept deliberately small to minimize future size.
//! The ScriptContext lives in a static (see ducky/mod.rs).

use heapless::{String, Vec};
use defmt::*;

use crate::ducky::expr::{eval_expr, eval_condition, Value, randomize};
use crate::ducky::parser::{parse_line, jittered_delay};

fn upper64(s: &str) -> String<64> {
    let mut u: String<64> = String::new();
    for c in s.chars().take(64) { let _ = u.push(c.to_ascii_uppercase()); }
    u
}

// ---------------------------------------------------------------------------
// ScriptContext — kept small, lives in static storage
// ---------------------------------------------------------------------------

pub struct ScriptContext {
    pub vars:                 heapless::FnvIndexMap<String<16>, Value, 8>,
    pub funcs:                heapless::FnvIndexMap<String<16>, Vec<String<64>, 8>, 4>,
    pub defines:              heapless::FnvIndexMap<String<16>, String<32>, 4>,
    pub held_keys:            Vec<u8, 8>,
    pub default_delay:        u64,
    pub string_char_delay_ms: Option<u64>,
    pub active_layout:        String<8>,
    pub jitter_enabled:       bool,
    pub jitter_max_delay:     u64,
    pub host_os:              String<16>,
    pub led_state:            bool,
    pub saved_caps_lock:      bool,
    pub saved_num_lock:       bool,
    pub saved_scroll_lock:    bool,
    pub previous_line:        String<64>,
}

impl ScriptContext {
    pub fn new() -> Self {
        let mut active_layout: String<8> = String::new();
        let _ = active_layout.push_str("US");
        let mut host_os: String<16> = String::new();
        let _ = host_os.push_str("UNKNOWN");
        Self {
            vars: heapless::FnvIndexMap::new(),
            funcs: heapless::FnvIndexMap::new(),
            defines: heapless::FnvIndexMap::new(),
            held_keys: Vec::new(),
            default_delay: 0,
            string_char_delay_ms: None,
            active_layout,
            jitter_enabled: false,
            jitter_max_delay: 50,
            host_os,
            led_state: false,
            saved_caps_lock: false,
            saved_num_lock: false,
            saved_scroll_lock: false,
            previous_line: String::new(),
        }
    }

    pub fn get_var(&self, name: &str) -> Option<Value> {
        match name {
            // $loot.bin — reads loot.bin contents (truncated to 64 chars for expressions)
            // Use STRING $loot.bin to type the full contents
            "$loot.bin" => {
                static mut LOOT_VAR_CACHE: heapless::String<64> = heapless::String::new();
                // Return cached value — refreshed by LOAD_LOOT or STRING $loot.bin
                return Some(Value::Str(unsafe { LOOT_VAR_CACHE.clone() }));
            }
            "$_JITTER_ENABLED"   => return Some(Value::Bool(self.jitter_enabled)),
            "$_JITTER_MAX_DELAY" => return Some(Value::Int(self.jitter_max_delay as i32)),
            "$_HOST_OS"          => {
                let mut s: String<64> = String::new();
                let _ = s.push_str(self.host_os.as_str());
                return Some(Value::Str(s));
            }
            "$_ACTIVE_LAYOUT"    => {
                let mut s: String<64> = String::new();
                let _ = s.push_str(self.active_layout.as_str());
                return Some(Value::Str(s));
            }
            _ => {}
        }
        let mut key: String<16> = String::new();
        for c in name.chars().take(16) { let _ = key.push(c); }
        self.vars.get(&key).cloned()
    }

    pub fn set_var(&mut self, name: &str, val: Value) {
        match name {
            "$_JITTER_ENABLED"    => { if let Value::Bool(b) = &val { self.jitter_enabled = *b; } return; }
            "$_JITTER_MAX_DELAY"  => { self.jitter_max_delay = val.as_int() as u64; return; }
            "$_HOST_OS"           => { self.host_os.clear(); let _ = self.host_os.push_str(val.as_str().as_str()); return; }
            "$_EXFIL_MODE_ENABLED" => { crate::exfil::EXFIL_MODE_ENABLED.signal(val.as_bool()); return; }
            _ => {}
        }
        let mut key: String<16> = String::new();
        for c in name.chars().take(16) { let _ = key.push(c); }
        let _ = self.vars.insert(key, val);
    }

    pub fn set_var_str(&mut self, name: &str, val: &str) {
        let mut s: String<64> = String::new();
        let _ = s.push_str(val);
        self.set_var(name, Value::Str(s));
    }
    pub fn set_var_bool(&mut self, name: &str, val: bool) { self.set_var(name, Value::Bool(val)); }
    pub fn set_var_int(&mut self, name: &str, val: i32) { self.set_var(name, Value::Int(val)); }
}

// ---------------------------------------------------------------------------
// ExecResult
// ---------------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
pub enum ExecResult { Continue, Restart, Stop }

// ---------------------------------------------------------------------------
// Pre-passes
// ---------------------------------------------------------------------------

pub fn collect_defines<'a>(lines: &[&'a str], ctx: &mut ScriptContext) -> Vec<String<64>, 64> {
    let mut out: Vec<String<64>, 64> = Vec::new();
    for &line in lines {
        let stripped = line.trim();
        if upper64(stripped).starts_with("DEFINE ") {
            let rest = stripped[7..].trim();
            if let Some(sp) = rest.find(' ') {
                let mut name: String<16> = String::new();
                for c in rest[..sp].chars().take(16) { let _ = name.push(c.to_ascii_uppercase()); }
                let mut val: String<32> = String::new();
                let _ = val.push_str(&rest[sp+1..].trim()[..rest[sp+1..].trim().len().min(32)]);
                let _ = ctx.defines.insert(name, val);
            }
        } else {
            let mut s: String<64> = String::new();
            let _ = s.push_str(&line[..line.len().min(64)]);
            let _ = out.push(s);
        }
    }
    out
}

pub fn collect_functions(lines: &[String<64>], ctx: &mut ScriptContext) -> Vec<String<64>, 64> {
    let mut out: Vec<String<64>, 64> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let u = upper64(lines[i].as_str().trim());
        if u.starts_with("FUNCTION ") && u.ends_with("()") {
            let fname_str = &u.as_str()[9..u.len()-2];
            let mut fname: String<16> = String::new();
            for c in fname_str.chars().take(16) { let _ = fname.push(c); }
            let mut body: Vec<String<64>, 8> = Vec::new();
            i += 1;
            while i < lines.len() {
                let bl = lines[i].as_str().trim();
                if upper64(bl).as_str() == "END_FUNCTION" { i += 1; break; }
                let mut s: String<64> = String::new();
                let _ = s.push_str(&bl[..bl.len().min(64)]);
                let _ = body.push(s);
                i += 1;
            }
            let _ = ctx.funcs.insert(fname, body);
        } else {
            let _ = out.push(lines[i].clone());
            i += 1;
        }
    }
    out
}

fn apply_defines(line: &str, ctx: &ScriptContext) -> String<64> {
    let mut result: String<64> = String::new();
    let _ = result.push_str(&line[..line.len().min(64)]);
    if ctx.defines.is_empty() { return result; }
    for (name, val) in ctx.defines.iter() {
        if let Some(idx) = result.as_str().find(name.as_str()) {
            let mut new: String<64> = String::new();
            let _ = new.push_str(&result.as_str()[..idx]);
            let _ = new.push_str(val.as_str());
            let rest = &result.as_str()[idx + name.len()..];
            let _ = new.push_str(&rest[..rest.len().min(64 - new.len())]);
            result = new;
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Main executor — iterative, minimal future size
// ---------------------------------------------------------------------------

impl ScriptContext {
    pub async fn execute<'a>(&mut self, lines: &[&'a str]) -> ExecResult {
        let mut i = 0;
        while i < lines.len() {
            let line_owned = apply_defines(lines[i], self);
            let stripped   = line_owned.as_str().trim();
            let upper      = upper64(stripped);
            let u          = upper.as_str();

            if u == "RESTART_PAYLOAD" { return ExecResult::Restart; }
            if u == "STOP_PAYLOAD"    { return ExecResult::Stop; }

            if u.starts_with("REPEAT ") {
                let count = eval_expr(stripped[7..].trim(), self).as_int().max(0) as usize;
                let prev = self.previous_line.clone();
                for _ in 0..count {
                    parse_line(prev.as_str(), self).await;
                    jittered_delay(self.default_delay, self).await;
                }
                i += 1; jittered_delay(self.default_delay, self).await; continue;
            }

            if u == "RANDOMIZE" { randomize(); i += 1; continue; }

            if u.starts_with("VAR ") {
                handle_assignment(stripped[4..].trim(), self);
                i += 1; jittered_delay(self.default_delay, self).await; continue;
            }

            if stripped.starts_with('$') && stripped.contains('=') {
                handle_assignment(stripped, self);
                i += 1; jittered_delay(self.default_delay, self).await; continue;
            }

            if u.starts_with("WHILE ") {
                let cond_str = stripped[6..].trim();
                let (body, end_idx) = extract_block(lines, i + 1, "WHILE", "END_WHILE");
                while eval_condition(cond_str, self) {
                    for bl in &body {
                        let line2 = apply_defines(bl.as_str(), self);
                        parse_line(line2.as_str().trim(), self).await;
                        jittered_delay(self.default_delay, self).await;
                    }
                }
                i = end_idx + 1; jittered_delay(self.default_delay, self).await; continue;
            }

            if u.starts_with("FOR ") {
                i = self.handle_for(lines, i).await;
                jittered_delay(self.default_delay, self).await; continue;
            }

            if u.starts_with("IF ") || u == "IF" {
                i = self.handle_if(lines, i).await;
                jittered_delay(self.default_delay, self).await; continue;
            }

            if stripped.ends_with("()") && !stripped.contains(' ') {
                let fname_str = upper64(&stripped[..stripped.len()-2]);
                let mut fname_key: String<16> = String::new();
                for c in fname_str.chars().take(16) { let _ = fname_key.push(c); }
                if let Some(body) = self.funcs.get(&fname_key).cloned() {
                    for bl in &body {
                        let line2 = apply_defines(bl.as_str(), self);
                        parse_line(line2.as_str().trim(), self).await;
                        jittered_delay(self.default_delay, self).await;
                    }
                }
                i += 1; jittered_delay(self.default_delay, self).await; continue;
            }

            parse_line(stripped, self).await;
            let mut prev: String<64> = String::new();
            let _ = prev.push_str(stripped);
            self.previous_line = prev;
            i += 1;
            jittered_delay(self.default_delay, self).await;
        }
        ExecResult::Continue
    }

    async fn handle_for<'a>(&mut self, lines: &[&'a str], start: usize) -> usize {
        let line = lines[start].trim();
        let rest = &line[4..];
        let upper_rest = upper64(rest);
        let from_idx = match upper_rest.as_str().find(" FROM ") {
            Some(i) => i, None => { warn!("[FOR] missing FROM"); return start + 1; }
        };
        let var_name = rest[..from_idx].trim();
        let after_from = &rest[from_idx + 6..];
        let upper_after = upper64(after_from);
        let to_idx = match upper_after.as_str().find(" TO ") {
            Some(i) => i, None => { warn!("[FOR] missing TO"); return start + 1; }
        };
        let start_expr = &after_from[..to_idx];
        let after_to   = &after_from[to_idx + 4..];
        let upper_to   = upper64(after_to);
        let (end_expr, step_expr) = if let Some(si) = upper_to.as_str().find(" STEP ") {
            (&after_to[..si], &after_to[si + 6..])
        } else { (after_to, "1") };

        let start_val = eval_expr(start_expr.trim(), self).as_int();
        let end_val   = eval_expr(end_expr.trim(),   self).as_int();
        let step_val  = eval_expr(step_expr.trim(),  self).as_int();
        if step_val == 0 { return start + 1; }

        let (body, end_idx) = extract_block(lines, start + 1, "FOR", "END_FOR");
        let mut val = start_val;
        loop {
            if step_val > 0 && val > end_val { break; }
            if step_val < 0 && val < end_val { break; }
            self.set_var_int(var_name, val);
            for bl in &body {
                let line2 = apply_defines(bl.as_str(), self);
                parse_line(line2.as_str().trim(), self).await;
                jittered_delay(self.default_delay, self).await;
            }
            val = val.wrapping_add(step_val);
        }
        end_idx + 1
    }

    async fn handle_if<'a>(&mut self, lines: &[&'a str], start: usize) -> usize {
        let mut i = start;
        let mut executed = false;
        loop {
            if i >= lines.len() { break; }
            let line  = lines[i].trim();
            let upper = upper64(line);
            let u     = upper.as_str();
            if u.starts_with("IF ") || u == "IF" {
                let cond_raw = if u.starts_with("IF ") { &line[3..] } else { "" };
                let cond = if upper64(cond_raw).ends_with(" THEN") { &cond_raw[..cond_raw.len()-5] } else { cond_raw };
                let (body, end) = extract_if_clause(lines, i + 1);
                if !executed && eval_condition(cond.trim(), self) {
                    for bl in &body {
                        let line2 = apply_defines(bl.as_str(), self);
                        parse_line(line2.as_str().trim(), self).await;
                        jittered_delay(self.default_delay, self).await;
                    }
                    executed = true;
                }
                i = end;
            } else if u.starts_with("ELSE IF ") {
                let cond_raw = &line[8..];
                let cond = if upper64(cond_raw).ends_with(" THEN") { &cond_raw[..cond_raw.len()-5] } else { cond_raw };
                let (body, end) = extract_if_clause(lines, i + 1);
                if !executed && eval_condition(cond.trim(), self) {
                    for bl in &body {
                        let line2 = apply_defines(bl.as_str(), self);
                        parse_line(line2.as_str().trim(), self).await;
                        jittered_delay(self.default_delay, self).await;
                    }
                    executed = true;
                }
                i = end;
            } else if u == "ELSE" {
                let (body, end) = extract_if_clause_no_else(lines, i + 1);
                if !executed {
                    for bl in &body {
                        let line2 = apply_defines(bl.as_str(), self);
                        parse_line(line2.as_str().trim(), self).await;
                        jittered_delay(self.default_delay, self).await;
                    }
                }
                i = end;
            } else if u == "END_IF" {
                i += 1; break;
            } else { i += 1; break; }
        }
        i
    }
}

// ---------------------------------------------------------------------------
// Block extraction
// ---------------------------------------------------------------------------

fn extract_block<'a>(lines: &[&'a str], start: usize, open_kw: &str, close_kw: &str)
    -> (Vec<String<64>, 32>, usize)
{
    let mut depth = 1usize;
    let mut body: Vec<String<64>, 32> = Vec::new();
    let mut i = start;
    let open_u  = upper64(open_kw);
    let close_u = upper64(close_kw);
    while i < lines.len() {
        let l = upper64(lines[i].trim());
        if l.starts_with(open_u.as_str()) { depth += 1; }
        if l.as_str() == close_u.as_str() {
            depth -= 1;
            if depth == 0 { return (body, i); }
        }
        let mut s: String<64> = String::new();
        let _ = s.push_str(&lines[i][..lines[i].len().min(64)]);
        let _ = body.push(s);
        i += 1;
    }
    (body, i)
}

fn extract_if_clause<'a>(lines: &[&'a str], start: usize) -> (Vec<String<64>, 32>, usize) {
    let mut depth = 0usize;
    let mut body: Vec<String<64>, 32> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let l = upper64(lines[i].trim());
        let u = l.as_str();
        if u.starts_with("IF ") || u == "IF" { depth += 1; }
        if u == "END_IF" { if depth == 0 { return (body, i); } depth -= 1; }
        if depth == 0 && (u == "ELSE" || u.starts_with("ELSE IF")) { return (body, i); }
        let mut s: String<64> = String::new();
        let _ = s.push_str(&lines[i][..lines[i].len().min(64)]);
        let _ = body.push(s);
        i += 1;
    }
    (body, i)
}

fn extract_if_clause_no_else<'a>(lines: &[&'a str], start: usize) -> (Vec<String<64>, 32>, usize) {
    let mut depth = 0usize;
    let mut body: Vec<String<64>, 32> = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let l = upper64(lines[i].trim());
        let u = l.as_str();
        if u.starts_with("IF ") || u == "IF" { depth += 1; }
        if u == "END_IF" { if depth == 0 { return (body, i); } depth -= 1; }
        let mut s: String<64> = String::new();
        let _ = s.push_str(&lines[i][..lines[i].len().min(64)]);
        let _ = body.push(s);
        i += 1;
    }
    (body, i)
}

// ---------------------------------------------------------------------------
// Assignment
// ---------------------------------------------------------------------------

fn handle_assignment(expr: &str, ctx: &mut ScriptContext) {
    let eq_idx = match expr.find('=') { Some(i) => i, None => return };
    let lhs = expr[..eq_idx].trim();
    let rhs = expr[eq_idx+1..].trim();
    if !lhs.starts_with('$') { return; }
    let val = eval_expr(rhs, ctx);
    ctx.set_var(lhs, val);
}
