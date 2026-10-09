//! AVM1 bytecode generation (the Flash 8 action format)
//! from the ActionScript 2 tree.

use super::parse::*;
use std::collections::HashMap;

/// Movie context: scenes (name, first global frame) for
/// `gotoAndPlay("Scene 2", 1)`, the document's .as scripts (for
/// `#include` and classes) and the fully qualified names of their classes.
#[derive(Clone, Debug, Default)]
pub struct Ctx {
    pub scenes: Vec<(String, u32)>,
    pub scripts: Vec<(String, String)>,
    pub classes: Vec<String>,
}

enum PushVal<'a> {
    Str(&'a str),
    Num(f64),
    Bool(bool),
    Null,
    Undef,
    Reg(u8),
}

#[derive(Default)]
struct Loop {
    breaks: Vec<usize>,
    conts: Vec<usize>,
    /// for…in: when leaving with break the name stack must be emptied.
    forin: bool,
    /// switch: allows break but not continue.
    switch: bool,
}

pub struct Gen<'c> {
    pub out: Vec<u8>,
    loops: Vec<Loop>,
    tmp: u8,
    line: usize,
    ctx: &'c Ctx,
    pub errors: Vec<AsError>,
    /// Functions with DefineFunction2 (class ones, so that `super` exists).
    pub fn2: bool,
    /// Imported names: short name → full path.
    pub aliases: HashMap<String, Vec<String>>,
}

const GOTO_FRAME: u8 = 0x81;
const GET_URL: u8 = 0x83;
const STORE_REG: u8 = 0x87;
const GOTO_LABEL: u8 = 0x8C;
const WITH: u8 = 0x94;
const PUSH: u8 = 0x96;
const JUMP: u8 = 0x99;
const GET_URL2: u8 = 0x9A;
const DEFINE_FUNCTION: u8 = 0x9B;
const IF: u8 = 0x9D;
const GOTO_FRAME2: u8 = 0x9F;

const POP: u8 = 0x17;
const GET_VAR: u8 = 0x1C;
const SET_VAR: u8 = 0x1D;
const GET_MEMBER: u8 = 0x4E;
const SET_MEMBER: u8 = 0x4F;
const NOT: u8 = 0x12;
const DUP: u8 = 0x4C;
const RETURN: u8 = 0x3E;

/// getProperty / setProperty properties.
const PROPS: [&str; 22] = [
    "_x", "_y", "_xscale", "_yscale", "_currentframe", "_totalframes", "_alpha", "_visible", "_width", "_height", "_rotation", "_target", "_framesloaded", "_name",
    "_droptarget", "_url", "_highquality", "_focusrect", "_soundbuftime", "_quality", "_xmouse", "_ymouse",
];

impl<'c> Gen<'c> {
    pub fn new(ctx: &'c Ctx) -> Gen<'c> {
        Gen { out: Vec::new(), loops: Vec::new(), tmp: 0, line: 1, ctx, errors: Vec::new(), fn2: false, aliases: HashMap::new() }
    }

    fn error(&mut self, msg: impl Into<String>) {
        self.errors.push(AsError { line: self.line, msg: msg.into() });
    }

    // ───────── Emission ─────────

    fn op(&mut self, code: u8) {
        self.out.push(code);
    }

    fn op_data(&mut self, code: u8, data: &[u8]) {
        self.out.push(code);
        self.out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        self.out.extend_from_slice(data);
    }

    fn push(&mut self, vals: &[PushVal]) {
        let mut d = Vec::new();
        for v in vals {
            match v {
                PushVal::Str(s) => {
                    d.push(0);
                    d.extend_from_slice(s.as_bytes());
                    d.push(0);
                }
                PushVal::Num(n) => {
                    if n.fract() == 0.0 && *n >= i32::MIN as f64 && *n <= i32::MAX as f64 && !(*n == 0.0 && n.is_sign_negative()) {
                        d.push(7);
                        d.extend_from_slice(&(*n as i32).to_le_bytes());
                    } else {
                        // AVM1 double: the two 32-bit halves are swapped.
                        let b = n.to_le_bytes();
                        d.push(6);
                        d.extend_from_slice(&b[4..8]);
                        d.extend_from_slice(&b[0..4]);
                    }
                }
                PushVal::Bool(b) => {
                    d.push(5);
                    d.push(*b as u8);
                }
                PushVal::Null => d.push(2),
                PushVal::Undef => d.push(3),
                PushVal::Reg(r) => {
                    d.push(4);
                    d.push(*r);
                }
            }
        }
        self.op_data(PUSH, &d);
    }

    fn push_str(&mut self, s: &str) {
        self.push(&[PushVal::Str(s)]);
    }

    fn store(&mut self, r: u8) {
        self.op_data(STORE_REG, &[r]);
    }

    /// Jump (or conditional jump) to a label resolved later.
    fn jump_fwd(&mut self, code: u8) -> usize {
        self.op_data(code, &[0, 0]);
        self.out.len() - 2
    }

    fn patch(&mut self, at: usize, target: usize) {
        let off = target as i64 - (at as i64 + 2);
        self.out[at..at + 2].copy_from_slice(&(off as i16).to_le_bytes());
    }

    fn jump_back(&mut self, code: u8, target: usize) {
        let at = self.jump_fwd(code);
        self.patch(at, target);
    }

    fn temp(&mut self) -> u8 {
        let r = self.tmp % 4;
        self.tmp = self.tmp.wrapping_add(1);
        r
    }

    // ───────── Statements ─────────

    /// import a.b.C (or a.b.*): `C` becomes a.b.C.
    pub fn import(&mut self, path: &str) {
        if let Some(pkg) = path.strip_suffix(".*") {
            for c in &self.ctx.classes {
                if c.rsplit_once('.').map_or(false, |(p, _)| p == pkg) {
                    let short = c.rsplit('.').next().unwrap().to_string();
                    self.aliases.insert(short, c.split('.').map(String::from).collect());
                }
            }
        } else if path.contains('.') {
            let short = path.rsplit('.').next().unwrap().to_string();
            self.aliases.insert(short, path.split('.').map(String::from).collect());
        }
    }

    pub fn stmts(&mut self, v: &[Stmt]) {
        for s in v {
            if let Stmt::Import(p, _) = s {
                self.import(p);
            }
        }
        // Named functions are defined first (as Flash does).
        for s in v {
            if let Stmt::Func(n, p, b) = s {
                self.define_function(n, p, b);
            }
        }
        for s in v {
            if !matches!(s, Stmt::Func(..)) {
                self.stmt(s);
            }
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Empty => {}
            Stmt::Expr(e, line) => {
                self.line = *line;
                self.effect(e);
            }
            Stmt::Var(v) => {
                for (n, init) in v {
                    match init {
                        Some(e) => {
                            self.push_str(n);
                            self.value(e);
                            self.op(0x3C); // DefineLocal
                        }
                        None => {
                            self.push_str(n);
                            self.op(0x41); // DefineLocal2
                        }
                    }
                }
            }
            Stmt::Block(v) => self.stmts(v),
            Stmt::If(c, t, e) => {
                self.value(c);
                self.op(NOT);
                let to_else = self.jump_fwd(IF);
                self.stmt(t);
                match e {
                    Some(e) => {
                        let to_end = self.jump_fwd(JUMP);
                        let here = self.out.len();
                        self.patch(to_else, here);
                        self.stmt(e);
                        let end = self.out.len();
                        self.patch(to_end, end);
                    }
                    None => {
                        let here = self.out.len();
                        self.patch(to_else, here);
                    }
                }
            }
            Stmt::While(c, b) => {
                let top = self.out.len();
                self.value(c);
                self.op(NOT);
                let exit = self.jump_fwd(IF);
                self.loops.push(Loop::default());
                self.stmt(b);
                let l = self.loops.pop().unwrap();
                self.jump_back(JUMP, top);
                let end = self.out.len();
                self.patch(exit, end);
                self.close_loop(l, top, end);
            }
            Stmt::DoWhile(b, c) => {
                let top = self.out.len();
                self.loops.push(Loop::default());
                self.stmt(b);
                let l = self.loops.pop().unwrap();
                let cont = self.out.len();
                self.value(c);
                self.jump_back(IF, top);
                let end = self.out.len();
                self.close_loop(l, cont, end);
            }
            Stmt::For(init, c, upd, b) => {
                if let Some(i) = init {
                    self.stmt(i);
                }
                let top = self.out.len();
                let exit = c.as_ref().map(|c| {
                    self.value(c);
                    self.op(NOT);
                    self.jump_fwd(IF)
                });
                self.loops.push(Loop::default());
                self.stmt(b);
                let l = self.loops.pop().unwrap();
                let cont = self.out.len();
                if let Some(u) = upd {
                    self.effect(u);
                }
                self.jump_back(JUMP, top);
                let end = self.out.len();
                if let Some(x) = exit {
                    self.patch(x, end);
                }
                self.close_loop(l, cont, end);
            }
            Stmt::ForIn(is_var, target, obj, b) => {
                self.value(obj);
                self.op(0x55); // Enumerate2: null and then the names
                let top = self.out.len();
                let r = self.temp();
                self.store(r);
                self.push(&[PushVal::Null]);
                self.op(0x66); // StrictEquals
                let exit = self.jump_fwd(IF);
                // Assign the name to the loop variable.
                match target {
                    Expr::Ident(n) => {
                        self.push(&[PushVal::Str(n), PushVal::Reg(r)]);
                        self.op(if *is_var { 0x3C } else { SET_VAR });
                    }
                    t => self.assign_from_reg(t, r),
                }
                self.loops.push(Loop { forin: true, ..Default::default() });
                self.stmt(b);
                let l = self.loops.pop().unwrap();
                self.jump_back(JUMP, top);
                let end = self.out.len();
                self.patch(exit, end);
                // break inside for…in: discard the remaining names up to the null.
                if !l.breaks.is_empty() {
                    let skip = self.jump_fwd(JUMP);
                    let flush = self.out.len();
                    self.push(&[PushVal::Null]);
                    self.op(0x66);
                    self.op(NOT);
                    self.jump_back(IF, flush);
                    let after = self.out.len();
                    self.patch(skip, after);
                    for b in l.breaks {
                        self.patch(b, flush);
                    }
                }
                for c in l.conts {
                    self.patch(c, top);
                }
            }
            Stmt::Return(e) => {
                match e {
                    Some(e) => self.value(e),
                    None => self.push(&[PushVal::Undef]),
                }
                self.op(RETURN);
            }
            Stmt::Break(line) => {
                self.line = *line;
                let forin = self.loops.last().map(|l| l.forin);
                match forin {
                    None => self.error("«break» fuera de un bucle o de un switch"),
                    Some(_) => {
                        let j = self.jump_fwd(JUMP);
                        self.loops.last_mut().unwrap().breaks.push(j);
                    }
                }
            }
            Stmt::Continue(line) => {
                self.line = *line;
                let Some(i) = self.loops.iter().rposition(|l| !l.switch) else {
                    self.error("«continue» fuera de un bucle");
                    return;
                };
                let j = self.jump_fwd(JUMP);
                self.loops[i].conts.push(j);
            }
            Stmt::Switch(d, cases) => {
                self.value(d);
                let r = self.temp();
                self.store(r);
                self.op(POP);
                let mut to_case = Vec::new();
                for (test, _) in cases {
                    if let Some(t) = test {
                        self.push(&[PushVal::Reg(r)]);
                        self.value(t);
                        self.op(0x66);
                        to_case.push(Some(self.jump_fwd(IF)));
                    } else {
                        to_case.push(None);
                    }
                }
                let to_default = self.jump_fwd(JUMP);
                self.loops.push(Loop { switch: true, ..Default::default() });
                let mut default_at = None;
                for (k, (test, body)) in cases.iter().enumerate() {
                    let here = self.out.len();
                    match to_case[k] {
                        Some(j) => self.patch(j, here),
                        None => default_at = Some(here),
                    }
                    let _ = test;
                    self.stmts(body);
                }
                let l = self.loops.pop().unwrap();
                let end = self.out.len();
                self.patch(to_default, default_at.unwrap_or(end));
                for b in l.breaks {
                    self.patch(b, end);
                }
                // continue inside a switch belongs to the outer loop.
                if let Some(outer) = self.loops.last_mut() {
                    outer.conts.extend(l.conts);
                }
            }
            Stmt::Func(n, p, b) => self.define_function(n, p, b),
            Stmt::With(o, b) => {
                self.value(o);
                let at = self.jump_fwd(WITH);
                let start = self.out.len();
                self.stmt(b);
                let size = (self.out.len() - start) as u16;
                self.out[at..at + 2].copy_from_slice(&size.to_le_bytes());
            }
            Stmt::On(_, _, line) | Stmt::OnClipEvent(_, _, line) => {
                self.line = *line;
                self.error("Los controladores on y onClipEvent van en las acciones de un botón o un clip de película, no en un fotograma");
            }
            Stmt::Import(..) => {}
            Stmt::Throw(e, line) => {
                self.line = *line;
                self.value(e);
                self.op(0x2A);
            }
            Stmt::Extends(sub, sup) => {
                self.value(sub);
                self.value(sup);
                self.op(0x69);
            }
            Stmt::Try(body, catch, fin) => {
                // ActionTry: flags, sizes of the three blocks and the catch variable name.
                let mut d = vec![catch.is_some() as u8 | (fin.is_some() as u8) << 1];
                d.extend_from_slice(&[0; 6]);
                if let Some((n, _)) = catch {
                    d.extend_from_slice(n.as_bytes());
                }
                d.push(0);
                self.op_data(0x8F, &d);
                let sizes = self.out.len() - d.len() + 1;
                let block = |g: &mut Self, v: Option<&Vec<Stmt>>, at: usize| {
                    let start = g.out.len();
                    if let Some(v) = v {
                        g.stmts(v);
                    }
                    let n = (g.out.len() - start) as u16;
                    g.out[sizes + at..sizes + at + 2].copy_from_slice(&n.to_le_bytes());
                };
                block(self, Some(body), 0);
                block(self, catch.as_ref().map(|c| &c.1), 2);
                block(self, fin.as_ref(), 4);
            }
        }
    }

    fn close_loop(&mut self, l: Loop, cont: usize, end: usize) {
        for b in l.breaks {
            self.patch(b, end);
        }
        for c in l.conts {
            self.patch(c, cont);
        }
    }

    fn define_function(&mut self, name: &str, params: &[String], body: &[Stmt]) {
        let mut d = Vec::new();
        d.extend_from_slice(name.as_bytes());
        d.push(0);
        d.extend_from_slice(&(params.len() as u16).to_le_bytes());
        if self.fn2 {
            // DefineFunction2: own registers (temporaries 0…3), this,
            // arguments and super as variables, parameters by name.
            d.push(5);
            d.extend_from_slice(&[0, 0]);
            for p in params {
                d.push(0);
                d.extend_from_slice(p.as_bytes());
                d.push(0);
            }
        } else {
            for p in params {
                d.extend_from_slice(p.as_bytes());
                d.push(0);
            }
        }
        d.extend_from_slice(&[0, 0]); // body size
        self.op_data(if self.fn2 { 0x8E } else { DEFINE_FUNCTION }, &d);
        let size_at = self.out.len() - 2;
        let start = self.out.len();
        let loops = std::mem::take(&mut self.loops);
        self.stmts(body);
        self.loops = loops;
        let size = (self.out.len() - start) as u16;
        self.out[size_at..size_at + 2].copy_from_slice(&size.to_le_bytes());
    }

    // ───────── Expressions ─────────

    /// An expression whose value is not used (a statement).
    fn effect(&mut self, e: &Expr) {
        match e {
            Expr::Assign(op, t, v) => self.assign(*op, t, v, false),
            Expr::Update { inc, target, .. } => self.update(*inc, true, target, false),
            Expr::Call(f, a) => {
                if !self.builtin(f, a, false) {
                    self.call(f, a);
                    self.op(POP);
                }
            }
            Expr::Comma(v) => {
                for x in v {
                    self.effect(x);
                }
            }
            _ => {
                self.value(e);
                self.op(POP);
            }
        }
    }

    /// Leaves the expression's value on the stack.
    fn value(&mut self, e: &Expr) {
        match e {
            Expr::Num(n) => self.push(&[PushVal::Num(*n)]),
            Expr::Str(s) => self.push(&[PushVal::Str(s)]),
            Expr::Bool(b) => self.push(&[PushVal::Bool(*b)]),
            Expr::Null => self.push(&[PushVal::Null]),
            Expr::Undef => self.push(&[PushVal::Undef]),
            Expr::Ident(n) if self.aliases.contains_key(n) => {
                let path = self.aliases[n].clone();
                self.push_str(&path[0]);
                self.op(GET_VAR);
                for k in &path[1..] {
                    self.push_str(k);
                    self.op(GET_MEMBER);
                }
            }
            Expr::Ident(n) => {
                self.push_str(n);
                self.op(GET_VAR);
            }
            Expr::Member(o, k) => {
                self.value(o);
                self.push_str(k);
                self.op(GET_MEMBER);
            }
            Expr::Index(o, i) => {
                self.value(o);
                self.value(i);
                self.op(GET_MEMBER);
            }
            Expr::Call(f, a) => {
                if !self.builtin(f, a, true) {
                    self.call(f, a);
                }
            }
            Expr::New(c, a) => {
                self.args(a);
                match &**c {
                    Expr::Ident(n) if self.aliases.contains_key(n) => {
                        let mut path = self.aliases[n].clone();
                        let last = path.pop().unwrap();
                        self.value(&Expr::Ident(path[0].clone()));
                        for k in &path[1..] {
                            self.push_str(k);
                            self.op(GET_MEMBER);
                        }
                        self.push_str(&last);
                        self.op(0x53);
                    }
                    Expr::Ident(n) => {
                        self.push_str(n);
                        self.op(0x40); // NewObject
                    }
                    Expr::Member(o, k) => {
                        self.value(o);
                        self.push_str(k);
                        self.op(0x53); // NewMethod
                    }
                    other => {
                        self.value(other);
                        self.push(&[PushVal::Undef]);
                        self.op(0x53);
                    }
                }
            }
            Expr::Unary(op, x) => match op {
                UnOp::Not => {
                    self.value(x);
                    self.op(NOT);
                }
                UnOp::Neg => {
                    self.push(&[PushVal::Num(0.0)]);
                    self.value(x);
                    self.op(0x0B);
                }
                UnOp::Plus => {
                    self.value(x);
                    self.op(0x4A);
                }
                UnOp::BitNot => {
                    self.value(x);
                    self.push(&[PushVal::Num(-1.0)]);
                    self.op(0x62);
                }
                UnOp::TypeOf => {
                    self.value(x);
                    self.op(0x44);
                }
                UnOp::Void => {
                    self.value(x);
                    self.op(POP);
                    self.push(&[PushVal::Undef]);
                }
            },
            Expr::Binary(op, a, b) => {
                if *op == BinOp::In {
                    self.error("El operador «in» solo se puede usar en for…in");
                }
                self.value(a);
                self.value(b);
                self.binop(*op);
            }
            Expr::Logical(and, a, b) => {
                self.value(a);
                self.op(DUP);
                if *and {
                    self.op(NOT);
                }
                let end = self.jump_fwd(IF);
                self.op(POP);
                self.value(b);
                let here = self.out.len();
                self.patch(end, here);
            }
            Expr::Cond(c, a, b) => {
                self.value(c);
                let to_a = self.jump_fwd(IF);
                self.value(b);
                let to_end = self.jump_fwd(JUMP);
                let here = self.out.len();
                self.patch(to_a, here);
                self.value(a);
                let end = self.out.len();
                self.patch(to_end, end);
            }
            Expr::Assign(op, t, v) => self.assign(*op, t, v, true),
            Expr::Update { inc, prefix, target } => self.update(*inc, *prefix, target, true),
            Expr::Array(v) => {
                for x in v.iter().rev() {
                    self.value(x);
                }
                self.push(&[PushVal::Num(v.len() as f64)]);
                self.op(0x42); // InitArray
            }
            Expr::Object(v) => {
                for (k, x) in v {
                    self.push_str(k);
                    self.value(x);
                }
                self.push(&[PushVal::Num(v.len() as f64)]);
                self.op(0x43); // InitObject
            }
            Expr::Func(name, p, b) => {
                // Function expression: without a name it leaves the function on the stack.
                self.define_function("", p, b);
                if let Some(n) = name {
                    // function f() {} as a value: it is also defined under that name.
                    let r = self.temp();
                    self.store(r);
                    self.push(&[PushVal::Str(n), PushVal::Reg(r)]);
                    self.op(SET_VAR);
                }
            }
            Expr::Delete(t) => match &**t {
                Expr::Member(o, k) => {
                    self.value(o);
                    self.push_str(k);
                    self.op(0x3A);
                }
                Expr::Index(o, i) => {
                    self.value(o);
                    self.value(i);
                    self.op(0x3A);
                }
                Expr::Ident(n) => {
                    self.push_str(n);
                    self.op(0x3B);
                }
                other => {
                    self.value(other);
                    self.op(POP);
                    self.push(&[PushVal::Bool(true)]);
                }
            },
            Expr::Comma(v) => {
                for (i, x) in v.iter().enumerate() {
                    if i + 1 < v.len() {
                        self.effect(x);
                    } else {
                        self.value(x);
                    }
                }
            }
        }
    }

    fn binop(&mut self, op: BinOp) {
        match op {
            BinOp::Add => self.op(0x47),
            BinOp::Sub => self.op(0x0B),
            BinOp::Mul => self.op(0x0C),
            BinOp::Div => self.op(0x0D),
            BinOp::Mod => self.op(0x3F),
            BinOp::Eq => self.op(0x49),
            BinOp::Ne => {
                self.op(0x49);
                self.op(NOT);
            }
            BinOp::StrictEq => self.op(0x66),
            BinOp::StrictNe => {
                self.op(0x66);
                self.op(NOT);
            }
            BinOp::Lt => self.op(0x48),
            BinOp::Gt => self.op(0x67),
            BinOp::Le => {
                self.op(0x67);
                self.op(NOT);
            }
            BinOp::Ge => {
                self.op(0x48);
                self.op(NOT);
            }
            BinOp::BitAnd => self.op(0x60),
            BinOp::BitOr => self.op(0x61),
            BinOp::BitXor => self.op(0x62),
            BinOp::Shl => self.op(0x63),
            BinOp::Shr => self.op(0x64),
            BinOp::UShr => self.op(0x65),
            BinOp::InstanceOf => self.op(0x54),
            BinOp::In => {}
        }
    }

    fn args(&mut self, a: &[Expr]) {
        for x in a.iter().rev() {
            self.value(x);
        }
        self.push(&[PushVal::Num(a.len() as f64)]);
    }

    fn call(&mut self, f: &Expr, a: &[Expr]) {
        self.args(a);
        match f {
            // super(…): the superclass constructor on this object.
            Expr::Ident(n) if n == "super" || self.aliases.contains_key(n) => {
                self.value(f);
                self.push(&[PushVal::Undef]);
                self.op(0x52);
            }
            Expr::Ident(n) => {
                self.push_str(n);
                self.op(0x3D); // CallFunction
            }
            Expr::Member(o, k) => {
                self.value(o);
                self.push_str(k);
                self.op(0x52); // CallMethod
            }
            Expr::Index(o, i) => {
                self.value(o);
                self.value(i);
                self.op(0x52);
            }
            other => {
                self.value(other);
                self.push(&[PushVal::Undef]);
                self.op(0x52);
            }
        }
    }

    /// Assignment: `t = v` or `t op= v`. With `want`, leaves the value on the stack.
    fn assign(&mut self, op: Option<BinOp>, t: &Expr, v: &Expr, want: bool) {
        let r = self.temp();
        match t {
            Expr::Ident(n) => {
                self.push_str(n);
                if let Some(op) = op {
                    self.push_str(n);
                    self.op(GET_VAR);
                    self.value(v);
                    self.binop(op);
                } else {
                    self.value(v);
                }
                if want {
                    self.store(r);
                }
                self.op(SET_VAR);
            }
            Expr::Member(o, k) => {
                self.value(o);
                if let Some(op) = op {
                    let ro = self.temp();
                    self.store(ro);
                    self.push(&[PushVal::Str(k), PushVal::Reg(ro), PushVal::Str(k)]);
                    self.op(GET_MEMBER);
                    self.value(v);
                    self.binop(op);
                } else {
                    self.push_str(k);
                    self.value(v);
                }
                if want {
                    self.store(r);
                }
                self.op(SET_MEMBER);
            }
            Expr::Index(o, i) => {
                self.value(o);
                if let Some(op) = op {
                    let (ro, ri) = (self.temp(), self.temp());
                    self.store(ro);
                    self.op(POP);
                    self.value(i);
                    self.store(ri);
                    self.op(POP);
                    self.push(&[PushVal::Reg(ro), PushVal::Reg(ri), PushVal::Reg(ro), PushVal::Reg(ri)]);
                    self.op(GET_MEMBER);
                    self.value(v);
                    self.binop(op);
                } else {
                    self.value(i);
                    self.value(v);
                }
                if want {
                    self.store(r);
                }
                self.op(SET_MEMBER);
            }
            _ => self.error("No se puede asignar a esta expresión"),
        }
        if want {
            self.push(&[PushVal::Reg(r)]);
        }
    }

    /// Assigns to `t` the value stored in a register (for…in with a property).
    fn assign_from_reg(&mut self, t: &Expr, r: u8) {
        match t {
            Expr::Member(o, k) => {
                self.value(o);
                self.push(&[PushVal::Str(k), PushVal::Reg(r)]);
                self.op(SET_MEMBER);
            }
            Expr::Index(o, i) => {
                self.value(o);
                self.value(i);
                self.push(&[PushVal::Reg(r)]);
                self.op(SET_MEMBER);
            }
            _ => self.error("Variable de bucle no válida"),
        }
    }

    /// ++ / -- (prefix or postfix).
    fn update(&mut self, inc: bool, prefix: bool, t: &Expr, want: bool) {
        let step = if inc { 0x50 } else { 0x51 };
        let r = self.temp();
        match t {
            Expr::Ident(n) => {
                self.push(&[PushVal::Str(n), PushVal::Str(n)]);
                self.op(GET_VAR);
                if want && !prefix {
                    self.store(r);
                }
                self.op(step);
                if want && prefix {
                    self.store(r);
                }
                self.op(SET_VAR);
            }
            Expr::Member(_, _) | Expr::Index(_, _) => {
                let (ro, rk) = (self.temp(), self.temp());
                match t {
                    Expr::Member(o, k) => {
                        self.value(o);
                        self.store(ro);
                        self.op(POP);
                        self.push(&[PushVal::Str(k)]);
                    }
                    Expr::Index(o, i) => {
                        self.value(o);
                        self.store(ro);
                        self.op(POP);
                        self.value(i);
                    }
                    _ => unreachable!(),
                }
                self.store(rk);
                self.op(POP);
                self.push(&[PushVal::Reg(ro), PushVal::Reg(rk), PushVal::Reg(ro), PushVal::Reg(rk)]);
                self.op(GET_MEMBER);
                if want && !prefix {
                    self.store(r);
                }
                self.op(step);
                if want && prefix {
                    self.store(r);
                }
                self.op(SET_MEMBER);
            }
            _ => self.error("«++» y «--» necesitan una variable o una propiedad"),
        }
        if want {
            self.push(&[PushVal::Reg(r)]);
        }
    }

    // ───────── Global functions with their own action ─────────

    /// Compiles the global functions that are actions in AVM1. Returns
    /// false if `f` is not one of them (it will be called as a normal function).
    fn builtin(&mut self, f: &Expr, a: &[Expr], want: bool) -> bool {
        let Expr::Ident(name) = f else { return false };
        let pushes_value;
        match (name.as_str(), a.len()) {
            ("stop", 0) | ("play", 0) | ("nextFrame", 0) | ("prevFrame", 0) | ("stopAllSounds", 0) | ("stopDrag", 0) => {
                self.op(match name.as_str() {
                    "stop" => 0x07,
                    "play" => 0x06,
                    "nextFrame" => 0x04,
                    "prevFrame" => 0x05,
                    "stopAllSounds" => 0x09,
                    _ => 0x28,
                });
                pushes_value = false;
            }
            ("gotoAndPlay", 1) | ("gotoAndStop", 1) | ("gotoAndPlay", 2) | ("gotoAndStop", 2) => {
                let play = name == "gotoAndPlay";
                let (scene, frame) = if a.len() == 2 { (Some(&a[0]), &a[1]) } else { (None, &a[0]) };
                let bias = match scene {
                    Some(Expr::Str(s)) => match self.ctx.scenes.iter().find(|(n, _)| n == s) {
                        Some((_, start)) => *start,
                        None => {
                            self.error(format!("No hay ninguna escena llamada «{s}»"));
                            0
                        }
                    },
                    Some(_) => {
                        self.error("El nombre de la escena debe ser una cadena");
                        0
                    }
                    None => 0,
                };
                match frame {
                    Expr::Num(n) if *n >= 1.0 => {
                        let idx = (bias + *n as u32 - 1).min(65535) as u16;
                        self.op_data(GOTO_FRAME, &idx.to_le_bytes());
                        self.op(if play { 0x06 } else { 0x07 });
                    }
                    Expr::Str(label) if bias == 0 => {
                        let mut d = label.as_bytes().to_vec();
                        d.push(0);
                        self.op_data(GOTO_LABEL, &d);
                        self.op(if play { 0x06 } else { 0x07 });
                    }
                    other => {
                        self.value(other);
                        if bias > 0 {
                            let mut d = vec![0x02 | play as u8];
                            d.extend_from_slice(&(bias.min(65535) as u16).to_le_bytes());
                            self.op_data(GOTO_FRAME2, &d);
                        } else {
                            self.op_data(GOTO_FRAME2, &[play as u8]);
                        }
                    }
                }
                pushes_value = false;
            }
            ("trace", 1) => {
                self.value(&a[0]);
                self.op(0x26);
                pushes_value = false;
            }
            ("getURL", 1..=3) => {
                let method = match a.get(2) {
                    Some(Expr::Str(m)) if m.eq_ignore_ascii_case("GET") => 1u8,
                    Some(Expr::Str(m)) if m.eq_ignore_ascii_case("POST") => 2,
                    Some(_) => {
                        self.error("El método de getURL debe ser \"GET\" o \"POST\"");
                        0
                    }
                    None => 0,
                };
                match (&a[0], a.get(1), method) {
                    (Expr::Str(u), None | Some(Expr::Str(_)), 0) => {
                        let w = match a.get(1) {
                            Some(Expr::Str(w)) => w.as_str(),
                            _ => "",
                        };
                        let mut d = u.as_bytes().to_vec();
                        d.push(0);
                        d.extend_from_slice(w.as_bytes());
                        d.push(0);
                        self.op_data(GET_URL, &d);
                    }
                    _ => {
                        self.value(&a[0]);
                        match a.get(1) {
                            Some(w) => self.value(w),
                            None => self.push_str(""),
                        }
                        self.op_data(GET_URL2, &[method << 6]);
                    }
                }
                pushes_value = false;
            }
            ("startDrag", 1..=6) => {
                if a.len() == 6 {
                    for x in &a[2..6] {
                        self.value(x);
                    }
                    self.push(&[PushVal::Num(1.0)]);
                } else {
                    self.push(&[PushVal::Num(0.0)]);
                }
                match a.get(1) {
                    Some(l) => self.value(l),
                    None => self.push(&[PushVal::Bool(false)]),
                }
                self.value(&a[0]);
                self.op(0x27);
                pushes_value = false;
            }
            ("duplicateMovieClip", 3) => {
                self.value(&a[0]);
                self.value(&a[1]);
                self.push(&[PushVal::Num(16384.0)]);
                self.value(&a[2]);
                self.op(0x47);
                self.op(0x24);
                pushes_value = false;
            }
            ("removeMovieClip", 1) => {
                self.value(&a[0]);
                self.op(0x25);
                pushes_value = false;
            }
            ("getProperty", 2) | ("setProperty", 3) => {
                let p = match &a[1] {
                    Expr::Ident(p) | Expr::Str(p) => PROPS.iter().position(|x| x == p),
                    _ => None,
                };
                let Some(p) = p else {
                    self.error("Propiedad desconocida (usa _x, _y, _alpha, _rotation…)");
                    return true;
                };
                self.value(&a[0]);
                self.push(&[PushVal::Num(p as f64)]);
                if name == "setProperty" {
                    self.value(&a[2]);
                    self.op(0x23);
                    pushes_value = false;
                } else {
                    self.op(0x22);
                    pushes_value = true;
                }
            }
            ("getTimer", 0) => {
                self.op(0x34);
                pushes_value = true;
            }
            ("random", 1) | ("int", 1) | ("chr", 1) | ("ord", 1) | ("length", 1) | ("eval", 1) | ("targetPath", 1) => {
                self.value(&a[0]);
                self.op(match name.as_str() {
                    "random" => 0x30,
                    "int" => 0x18,
                    "chr" => 0x33,
                    "ord" => 0x32,
                    "length" => 0x14,
                    "eval" => GET_VAR,
                    _ => 0x45,
                });
                pushes_value = true;
            }
            ("set", 2) => {
                self.value(&a[0]);
                self.value(&a[1]);
                self.op(SET_VAR);
                pushes_value = false;
            }
            _ => return false,
        }
        match (want, pushes_value) {
            (true, false) => self.push(&[PushVal::Undef]),
            (false, true) => self.op(POP),
            _ => {}
        }
        true
    }
}
