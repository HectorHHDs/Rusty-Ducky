//! ducky/expr.rs — DuckyScript expression evaluator
//! Mirrors _evalExpr(), _substituteVars() from duckyinpython.py.

use heapless::String;
use crate::ducky::executor::ScriptContext;

#[derive(Clone)]
pub enum Value {
    Int(i32),
    Bool(bool),
    Str(String<256>),
}

impl Value {
    pub fn as_int(&self) -> i32 {
        match self {
            Value::Int(n)  => *n,
            Value::Bool(b) => if *b { 1 } else { 0 },
            Value::Str(s)  => s.parse().unwrap_or(0),
        }
    }
    pub fn as_bool(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Int(n)  => *n != 0,
            Value::Str(s)  => !s.is_empty() && s.as_str() != "FALSE" && s.as_str() != "0",
        }
    }
    pub fn as_str(&self) -> String<256> {
        match self {
            Value::Str(s)  => s.clone(),
            Value::Int(n)  => { let mut s: String<256> = String::new(); let _ = core::fmt::write(&mut s, format_args!("{}", n)); s }
            Value::Bool(b) => { let mut s: String<256> = String::new(); let _ = s.push_str(if *b { "TRUE" } else { "FALSE" }); s }
        }
    }
}

static mut RNG_STATE: u32 = 0xDEADBEEF;

pub fn rand_range(min: i32, max: i32) -> i32 {
    if min >= max { return min; }
    let range = (max - min + 1) as u32;
    unsafe {
        RNG_STATE = RNG_STATE.wrapping_mul(1664525).wrapping_add(1013904223);
        min + (RNG_STATE % range) as i32
    }
}

pub fn randomize() {
    let seed = embassy_time::Instant::now().as_ticks() as u32;
    unsafe { RNG_STATE = seed ^ 0xDEADBEEF; }
}

pub fn substitute_vars(expr: &str, ctx: &ScriptContext) -> String<128> {
    let mut out: String<128> = String::new();
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i; i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') { i += 1; }
            let var_name = &expr[start..i];
            match ctx.get_var(var_name) {
                Some(val) => { let _ = out.push_str(val.as_str().as_str()); }
                None      => { let _ = out.push('0'); }
            }
        } else {
            let _ = out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

pub fn substitute_random_int(expr: &str) -> String<128> {
    let mut result: String<128> = String::new();
    let mut i = 0;
    let bytes = expr.as_bytes();
    while i < bytes.len() {
        // Case-insensitive match for RANDOM_INT(
        let rem = &expr[i..];
        let rem_upper = { let mut u: String<16> = String::new(); for c in rem.chars().take(11) { let _ = u.push(c.to_ascii_uppercase()); } u };
        if rem_upper.as_str() == "RANDOM_INT(" || (rem.len() >= 11 && rem_upper.as_str().starts_with("RANDOM_INT(")) {
            if let Some(end_off) = rem[11..].find(')') {
                let inner = &rem[11..11+end_off];
                let parts: heapless::Vec<&str, 2> = inner.splitn(2, ',').collect();
                if parts.len() == 2 {
                    let a = parts[0].trim().parse::<i32>().unwrap_or(0);
                    let b = parts[1].trim().parse::<i32>().unwrap_or(0);
                    let v = rand_range(a, b);
                    let _ = core::fmt::write(&mut result, format_args!("{}", v));
                    i += 11 + end_off + 1;
                    continue;
                }
            }
        }
        let _ = result.push(bytes[i] as char);
        i += 1;
    }
    result
}

pub fn eval_expr(expr: &str, ctx: &ScriptContext) -> Value {
    let after_vars = substitute_vars(expr.trim(), ctx);
    let after_rand = substitute_random_int(after_vars.as_str());
    let mut p = ExprParser::new(after_rand.as_str());
    p.parse_or()
}

pub fn eval_condition(expr: &str, ctx: &ScriptContext) -> bool {
    eval_expr(expr, ctx).as_bool()
}

struct ExprParser<'a> { src: &'a str, pos: usize }

impl<'a> ExprParser<'a> {
    fn new(src: &'a str) -> Self { Self { src, pos: 0 } }
    fn peek(&self) -> Option<u8> { self.src.as_bytes().get(self.pos).copied() }
    fn ws(&mut self) { while matches!(self.peek(), Some(b' ')|Some(b'\t')) { self.pos+=1; } }

    fn parse_or(&mut self) -> Value {
        let mut l = self.parse_and();
        loop { self.ws();
            if self.src.as_bytes().get(self.pos..self.pos+2) == Some(b"||") { self.pos+=2; let r=self.parse_and(); l=Value::Bool(l.as_bool()||r.as_bool()); } else { break; }
        } l
    }
    fn parse_and(&mut self) -> Value {
        let mut l = self.parse_eq();
        loop { self.ws();
            if self.src.as_bytes().get(self.pos..self.pos+2) == Some(b"&&") { self.pos+=2; let r=self.parse_eq(); l=Value::Bool(l.as_bool()&&r.as_bool()); } else { break; }
        } l
    }
    fn parse_eq(&mut self) -> Value {
        let mut l = self.parse_cmp();
        loop { self.ws(); let b=self.src.as_bytes();
            if b.get(self.pos..self.pos+2)==Some(b"==") { self.pos+=2; let r=self.parse_cmp(); l=Value::Bool(l.as_int()==r.as_int()); }
            else if b.get(self.pos..self.pos+2)==Some(b"!=") { self.pos+=2; let r=self.parse_cmp(); l=Value::Bool(l.as_int()!=r.as_int()); }
            else { break; }
        } l
    }
    fn parse_cmp(&mut self) -> Value {
        let mut l = self.parse_add();
        loop { self.ws(); let b=self.src.as_bytes();
            if b.get(self.pos..self.pos+2)==Some(b"<=") { self.pos+=2; let r=self.parse_add(); l=Value::Bool(l.as_int()<=r.as_int()); }
            else if b.get(self.pos..self.pos+2)==Some(b">=") { self.pos+=2; let r=self.parse_add(); l=Value::Bool(l.as_int()>=r.as_int()); }
            else if b.get(self.pos)==Some(&b'<') { self.pos+=1; let r=self.parse_add(); l=Value::Bool(l.as_int()<r.as_int()); }
            else if b.get(self.pos)==Some(&b'>') { self.pos+=1; let r=self.parse_add(); l=Value::Bool(l.as_int()>r.as_int()); }
            else { break; }
        } l
    }
    fn parse_add(&mut self) -> Value {
        let mut l = self.parse_mul();
        loop { self.ws();
            match self.peek() {
                Some(b'+') => { self.pos+=1; let r=self.parse_mul(); l=Value::Int(l.as_int().wrapping_add(r.as_int())); }
                Some(b'-') => { self.pos+=1; let r=self.parse_mul(); l=Value::Int(l.as_int().wrapping_sub(r.as_int())); }
                _ => break,
            }
        } l
    }
    fn parse_mul(&mut self) -> Value {
        let mut l = self.parse_unary();
        loop { self.ws();
            match self.peek() {
                Some(b'*') => { self.pos+=1; let r=self.parse_unary(); l=Value::Int(l.as_int().wrapping_mul(r.as_int())); }
                Some(b'/') => { self.pos+=1; let r=self.parse_unary(); let d=r.as_int(); l=Value::Int(if d!=0{l.as_int()/d}else{0}); }
                Some(b'%') => { self.pos+=1; let r=self.parse_unary(); let d=r.as_int(); l=Value::Int(if d!=0{l.as_int()%d}else{0}); }
                _ => break,
            }
        } l
    }
    fn parse_unary(&mut self) -> Value {
        self.ws();
        let b=self.src.as_bytes();
        if b.get(self.pos)==Some(&b'!') && b.get(self.pos+1)!=Some(&b'=') { self.pos+=1; let v=self.parse_unary(); return Value::Bool(!v.as_bool()); }
        if self.peek()==Some(b'-') { self.pos+=1; let v=self.parse_primary(); return Value::Int(-v.as_int()); }
        self.parse_primary()
    }
    fn parse_primary(&mut self) -> Value {
        self.ws();
        if self.peek()==Some(b'(') { self.pos+=1; let v=self.parse_or(); self.ws(); if self.peek()==Some(b')'){self.pos+=1;} return v; }
        let rem = &self.src[self.pos..];
        let up = { let mut u: heapless::String<6>=heapless::String::new(); for c in rem.chars().take(5){let _=u.push(c.to_ascii_uppercase());} u };
        if up.as_str().starts_with("TRUE")  && !rem.as_bytes().get(4).map(|b|b.is_ascii_alphanumeric()).unwrap_or(false) { self.pos+=4; return Value::Bool(true); }
        if up.as_str().starts_with("FALSE") && !rem.as_bytes().get(5).map(|b|b.is_ascii_alphanumeric()).unwrap_or(false) { self.pos+=5; return Value::Bool(false); }
        if self.peek().map(|b|b.is_ascii_digit()).unwrap_or(false) {
            let start=self.pos;
            while self.peek().map(|b|b.is_ascii_digit()).unwrap_or(false) { self.pos+=1; }
            return Value::Int(self.src[start..self.pos].parse().unwrap_or(0));
        }
        Value::Int(0)
    }
}
