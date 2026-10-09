//! ActionScript 2 parser: tokens and syntax tree.
//!
//! Covers what is written in the Flash 8 Actions panel: ECMAScript-style
//! statements and expressions (var, function, if, while, do, for,
//! for…in, switch, with, return, break, continue), object and array
//! literals, `new`, operators, type annotations (`var x:Number`, which are
//! accepted and ignored), try/catch/finally and throw, import, the
//! `on (…)` and `onClipEvent (…)` handlers and the classes of .as files.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct AsError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for AsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Línea {}: {}", self.line, self.msg)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Ident(String),
    Punct(&'static str),
    Eof,
}

#[derive(Clone, Debug)]
struct Token {
    t: Tok,
    line: usize,
    /// There was a line break before (for the optional semicolon).
    nl: bool,
}

const PUNCT: [&str; 49] = [
    ">>>=", "===", "!==", ">>>", "<<=", ">>=", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<",
    ">>", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "/", "%", "&", "|", "^", "!", "~", "?", ":", "=", ".", "@",
];

fn lex(src: &str) -> Result<Vec<Token>, AsError> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut line = 1;
    let mut nl = false;
    let mut out = Vec::new();
    let err = |line, msg: &str| Err(AsError { line, msg: msg.into() });
    while i < c.len() {
        let ch = c[i];
        if ch == '\n' {
            line += 1;
            nl = true;
            i += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                if c[i] == '\n' {
                    line += 1;
                    nl = true;
                }
                i += 1;
            }
            if i >= c.len() {
                return err(line, "Comentario sin cerrar");
            }
            i += 2;
            continue;
        }
        let start_line = line;
        let t = if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).map_or(false, |d| d.is_ascii_digit())) {
            let s = i;
            if ch == '0' && matches!(c.get(i + 1), Some('x') | Some('X')) {
                i += 2;
                while i < c.len() && c[i].is_ascii_hexdigit() {
                    i += 1;
                }
                let h: String = c[s + 2..i].iter().collect();
                Tok::Num(i64::from_str_radix(&h, 16).map_err(|_| AsError { line, msg: "Número hexadecimal no válido".into() })? as f64)
            } else {
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                    i += 1;
                }
                if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                    i += 1;
                    if i < c.len() && (c[i] == '+' || c[i] == '-') {
                        i += 1;
                    }
                    while i < c.len() && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let n: String = c[s..i].iter().collect();
                Tok::Num(n.parse().map_err(|_| AsError { line, msg: format!("Número no válido: {n}") })?)
            }
        } else if ch == '"' || ch == '\'' {
            let q = ch;
            i += 1;
            let mut s = String::new();
            loop {
                let Some(&d) = c.get(i) else { return err(start_line, "Cadena sin cerrar") };
                if d == '\n' {
                    return err(start_line, "Cadena sin cerrar");
                }
                i += 1;
                if d == q {
                    break;
                }
                if d == '\\' {
                    let Some(&e) = c.get(i) else { return err(line, "Cadena sin cerrar") };
                    i += 1;
                    match e {
                        'n' => s.push('\n'),
                        'r' => s.push('\r'),
                        't' => s.push('\t'),
                        'b' => s.push('\u{8}'),
                        'f' => s.push('\u{c}'),
                        'u' | 'x' => {
                            let n = if e == 'u' { 4 } else { 2 };
                            let h: String = c.get(i..i + n).map(|v| v.iter().collect()).unwrap_or_default();
                            let v = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32);
                            match v {
                                Some(v) => s.push(v),
                                None => return err(line, "Secuencia de escape no válida"),
                            }
                            i += n;
                        }
                        other => s.push(other),
                    }
                } else {
                    s.push(d);
                }
            }
            Tok::Str(s)
        } else if ch.is_alphabetic() || ch == '_' || ch == '$' {
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '$') {
                i += 1;
            }
            Tok::Ident(c[s..i].iter().collect())
        } else {
            let rest: String = c[i..(i + 4).min(c.len())].iter().collect();
            match PUNCT.iter().find(|p| rest.starts_with(**p)) {
                Some(p) => {
                    i += p.chars().count();
                    Tok::Punct(p)
                }
                None => return err(line, &format!("Carácter inesperado «{ch}»")),
            }
        };
        out.push(Token { t, line: start_line, nl });
        nl = false;
    }
    out.push(Token { t: Tok::Eof, line, nl: true });
    Ok(out)
}

// ───────────────────────── Tree ─────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    StrictEq,
    StrictNe,
    Lt,
    Gt,
    Le,
    Ge,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UShr,
    InstanceOf,
    In,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnOp {
    Not,
    Neg,
    Plus,
    BitNot,
    TypeOf,
    Void,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Num(f64),
    Str(String),
    Bool(bool),
    Null,
    Undef,
    Ident(String),
    Member(Box<Expr>, String),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    New(Box<Expr>, Vec<Expr>),
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `&&` (true) or `||` (false).
    Logical(bool, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Assign(Option<BinOp>, Box<Expr>, Box<Expr>),
    Update { inc: bool, prefix: bool, target: Box<Expr> },
    Array(Vec<Expr>),
    Object(Vec<(String, Expr)>),
    Func(Option<String>, Vec<String>, Vec<Stmt>),
    Delete(Box<Expr>),
    Comma(Vec<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Expr(Expr, usize),
    Var(Vec<(String, Option<Expr>)>),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    While(Expr, Box<Stmt>),
    DoWhile(Box<Stmt>, Expr),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    /// for (var k in o) / for (k in o) / for (o.p in o)
    ForIn(bool, Expr, Expr, Box<Stmt>),
    Block(Vec<Stmt>),
    Return(Option<Expr>),
    Break(usize),
    Continue(usize),
    Switch(Expr, Vec<(Option<Expr>, Vec<Stmt>)>),
    Func(String, Vec<String>, Vec<Stmt>),
    With(Expr, Box<Stmt>),
    Empty,
    /// `on (release, keyPress "<Enter>") { … }`
    On(Vec<Event>, Vec<Stmt>, usize),
    /// `onClipEvent (enterFrame) { … }`
    OnClipEvent(Vec<Event>, Vec<Stmt>, usize),
    /// try { … } catch (e) { … } finally { … }
    Try(Vec<Stmt>, Option<(String, Vec<Stmt>)>, Option<Vec<Stmt>>),
    Throw(Expr, usize),
    /// import a.b.C / import a.b.*
    Import(String, usize),
    /// Class inheritance (ActionExtends): subclass and superclass.
    Extends(Expr, Expr),
}

/// Class member.
#[derive(Clone, Debug, PartialEq)]
pub enum MemberKind {
    Var(Option<Expr>),
    Method(Vec<String>, Vec<Stmt>),
    Getter(Vec<Stmt>),
    Setter(Vec<String>, Vec<Stmt>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub name: String,
    pub is_static: bool,
    pub kind: MemberKind,
    pub line: usize,
}

/// Class (or interface) of an .as file.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassDef {
    /// Fully qualified name (with package).
    pub name: String,
    pub extends: Option<String>,
    pub interface: bool,
    pub imports: Vec<String>,
    pub members: Vec<Member>,
    pub line: usize,
}

/// Event of `on` / `onClipEvent`, with the `keyPress` key if any.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub name: String,
    pub key: Option<String>,
}

// ───────────────────────── Parser ─────────────────────────

struct Parser {
    t: Vec<Token>,
    i: usize,
}

type R<T> = Result<T, AsError>;

const RESERVED: [&str; 5] = ["class", "interface", "extends", "implements", "dynamic"];

impl Parser {
    fn peek(&self) -> &Tok {
        &self.t[self.i].t
    }
    fn line(&self) -> usize {
        self.t[self.i].line
    }
    fn next(&mut self) -> Token {
        let t = self.t[self.i].clone();
        if self.i < self.t.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn is(&self, p: &str) -> bool {
        matches!(self.peek(), Tok::Punct(q) if *q == p)
    }
    fn is_kw(&self, k: &str) -> bool {
        matches!(self.peek(), Tok::Ident(s) if s == k)
    }
    fn eat(&mut self, p: &str) -> bool {
        if self.is(p) {
            self.next();
            true
        } else {
            false
        }
    }
    fn eat_kw(&mut self, k: &str) -> bool {
        if self.is_kw(k) {
            self.next();
            true
        } else {
            false
        }
    }
    fn err<T>(&self, msg: impl Into<String>) -> R<T> {
        Err(AsError { line: self.line(), msg: msg.into() })
    }
    fn expect(&mut self, p: &str) -> R<()> {
        if self.eat(p) {
            Ok(())
        } else {
            let got = self.describe();
            self.err(format!("Se esperaba «{p}» y se encontró {got}"))
        }
    }
    fn describe(&self) -> String {
        match self.peek() {
            Tok::Num(n) => format!("el número {n}"),
            Tok::Str(s) => format!("la cadena \"{s}\""),
            Tok::Ident(s) => format!("«{s}»"),
            Tok::Punct(p) => format!("«{p}»"),
            Tok::Eof => "el final del script".into(),
        }
    }
    fn ident(&mut self) -> R<String> {
        match self.peek().clone() {
            Tok::Ident(s) => {
                self.next();
                Ok(s)
            }
            _ => {
                let got = self.describe();
                self.err(format!("Se esperaba un nombre y se encontró {got}"))
            }
        }
    }
    /// Optional type annotation (`:Number`, `:Array`, `:mx.controls.Button`).
    fn skip_type(&mut self) -> R<()> {
        if self.eat(":") {
            self.ident()?;
            while self.eat(".") {
                self.ident()?;
            }
        }
        Ok(())
    }
    /// End of statement: `;`, line break, `}` or end of script.
    fn end_stmt(&mut self) -> R<()> {
        if self.eat(";") || self.is("}") || matches!(self.peek(), Tok::Eof) || self.t[self.i].nl {
            return Ok(());
        }
        let got = self.describe();
        self.err(format!("Se esperaba «;» y se encontró {got}"))
    }

    fn program(&mut self) -> R<Vec<Stmt>> {
        let mut v = Vec::new();
        while !matches!(self.peek(), Tok::Eof) {
            v.push(self.stmt()?);
        }
        Ok(v)
    }

    fn block(&mut self) -> R<Vec<Stmt>> {
        self.expect("{")?;
        let mut v = Vec::new();
        while !self.is("}") {
            if matches!(self.peek(), Tok::Eof) {
                return self.err("Falta «}» de cierre");
            }
            v.push(self.stmt()?);
        }
        self.next();
        Ok(v)
    }

    fn events(&mut self, clip: bool) -> R<Vec<Event>> {
        self.expect("(")?;
        let mut v = Vec::new();
        loop {
            let name = self.ident()?;
            let key = if name == "keyPress" {
                match self.next().t {
                    Tok::Str(s) => Some(s),
                    _ => return self.err("keyPress necesita una tecla entre comillas, por ejemplo keyPress \"<Enter>\""),
                }
            } else {
                None
            };
            let ok_btn = ["press", "release", "releaseOutside", "rollOver", "rollOut", "dragOver", "dragOut", "keyPress"];
            let ok_clip = ["load", "unload", "enterFrame", "mouseDown", "mouseUp", "mouseMove", "keyDown", "keyUp", "data", "initialize", "construct"];
            let valid = if clip { ok_clip.contains(&name.as_str()) } else { ok_btn.contains(&name.as_str()) };
            if !valid {
                return self.err(format!("Evento desconocido «{name}»"));
            }
            v.push(Event { name, key });
            if !self.eat(",") {
                break;
            }
        }
        self.expect(")")?;
        Ok(v)
    }

    fn stmt(&mut self) -> R<Stmt> {
        let line = self.line();
        if self.is("{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.eat(";") {
            return Ok(Stmt::Empty);
        }
        if let Tok::Ident(k) = self.peek().clone() {
            if RESERVED.contains(&k.as_str()) {
                return self.err(format!("«{k}» no se puede usar en el panel Acciones (las clases van en archivos .as)"));
            }
            match k.as_str() {
                "try" => {
                    self.next();
                    let body = self.block()?;
                    let mut catch = None;
                    if self.eat_kw("catch") {
                        self.expect("(")?;
                        let n = self.ident()?;
                        self.skip_type()?;
                        self.expect(")")?;
                        catch = Some((n, self.block()?));
                        if self.is_kw("catch") {
                            return self.err("Solo se admite un bloque catch por try");
                        }
                    }
                    let fin = if self.eat_kw("finally") { Some(self.block()?) } else { None };
                    if catch.is_none() && fin.is_none() {
                        return self.err("«try» necesita un bloque catch o finally");
                    }
                    return Ok(Stmt::Try(body, catch, fin));
                }
                "throw" => {
                    self.next();
                    let e = self.expr()?;
                    self.end_stmt()?;
                    return Ok(Stmt::Throw(e, line));
                }
                "import" => {
                    self.next();
                    let path = self.dotted(true)?;
                    self.end_stmt()?;
                    return Ok(Stmt::Import(path, line));
                }
                "var" => {
                    self.next();
                    let v = self.var_list()?;
                    self.end_stmt()?;
                    return Ok(Stmt::Var(v));
                }
                "function" if matches!(self.t.get(self.i + 1).map(|t| &t.t), Some(Tok::Ident(_))) => {
                    self.next();
                    let name = self.ident()?;
                    let (params, body) = self.func_rest()?;
                    return Ok(Stmt::Func(name, params, body));
                }
                "if" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    let t = self.stmt()?;
                    let e = if self.eat_kw("else") { Some(Box::new(self.stmt()?)) } else { None };
                    return Ok(Stmt::If(c, Box::new(t), e));
                }
                "while" => {
                    self.next();
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::While(c, Box::new(self.stmt()?)));
                }
                "do" => {
                    self.next();
                    let b = self.stmt()?;
                    if !self.eat_kw("while") {
                        return self.err("Se esperaba «while» después del cuerpo de «do»");
                    }
                    self.expect("(")?;
                    let c = self.expr()?;
                    self.expect(")")?;
                    self.end_stmt()?;
                    return Ok(Stmt::DoWhile(Box::new(b), c));
                }
                "for" => return self.for_stmt(),
                "return" => {
                    self.next();
                    let e = if self.is(";") || self.is("}") || self.t[self.i].nl || matches!(self.peek(), Tok::Eof) { None } else { Some(self.expr()?) };
                    self.end_stmt()?;
                    return Ok(Stmt::Return(e));
                }
                "break" => {
                    self.next();
                    self.end_stmt()?;
                    return Ok(Stmt::Break(line));
                }
                "continue" => {
                    self.next();
                    self.end_stmt()?;
                    return Ok(Stmt::Continue(line));
                }
                "switch" => return self.switch_stmt(),
                "with" => {
                    self.next();
                    self.expect("(")?;
                    let o = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::With(o, Box::new(self.stmt()?)));
                }
                "on" if matches!(self.t.get(self.i + 1).map(|t| &t.t), Some(Tok::Punct("("))) => {
                    self.next();
                    let ev = self.events(false)?;
                    return Ok(Stmt::On(ev, self.block()?, line));
                }
                "onClipEvent" => {
                    self.next();
                    let ev = self.events(true)?;
                    return Ok(Stmt::OnClipEvent(ev, self.block()?, line));
                }
                _ => {}
            }
        }
        let e = self.expr()?;
        self.end_stmt()?;
        Ok(Stmt::Expr(e, line))
    }

    /// Dotted name (a.b.C); with `star`, allows a.b.* at the end.
    fn dotted(&mut self, star: bool) -> R<String> {
        let mut s = self.ident()?;
        while self.eat(".") {
            if star && self.eat("*") {
                s.push_str(".*");
                break;
            }
            s.push('.');
            s.push_str(&self.ident()?);
        }
        Ok(s)
    }

    /// Class file: imports and one class or interface.
    fn class_file(&mut self) -> R<ClassDef> {
        let mut imports = Vec::new();
        while self.eat_kw("import") {
            imports.push(self.dotted(true)?);
            self.end_stmt()?;
        }
        while self.eat_kw("dynamic") || self.eat_kw("intrinsic") {}
        let line = self.line();
        let interface = if self.eat_kw("interface") {
            true
        } else if self.eat_kw("class") {
            false
        } else {
            let got = self.describe();
            return self.err(format!("Se esperaba «class» o «interface» y se encontró {got}"));
        };
        let name = self.dotted(false)?;
        let mut extends = None;
        if self.eat_kw("extends") {
            let e = self.dotted(false)?;
            // An interface can extend several.
            while interface && self.eat(",") {
                self.dotted(false)?;
            }
            extends = Some(e);
        }
        if self.eat_kw("implements") {
            loop {
                self.dotted(false)?;
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect("{")?;
        let mut members = Vec::new();
        while !self.eat("}") {
            if matches!(self.peek(), Tok::Eof) {
                return self.err("Falta «}» de cierre de la clase");
            }
            if self.eat(";") {
                continue;
            }
            let mut is_static = false;
            loop {
                if self.eat_kw("static") {
                    is_static = true;
                } else if !(self.eat_kw("public") || self.eat_kw("private")) {
                    break;
                }
            }
            let line = self.line();
            if self.eat_kw("var") {
                for (name, init) in self.var_list()? {
                    members.push(Member { name, is_static, kind: MemberKind::Var(init), line });
                }
                self.end_stmt()?;
            } else if self.eat_kw("function") {
                // function get x() / function set x(v)
                let acc = match self.peek() {
                    Tok::Ident(s) if (s == "get" || s == "set") && matches!(self.t.get(self.i + 1).map(|t| &t.t), Some(Tok::Ident(_))) => {
                        let s = s.clone();
                        self.next();
                        Some(s)
                    }
                    _ => None,
                };
                let name = self.ident()?;
                if interface {
                    self.expect("(")?;
                    while !self.eat(")") {
                        if matches!(self.peek(), Tok::Eof) {
                            return self.err("Falta «)»");
                        }
                        self.next();
                    }
                    self.skip_type()?;
                    self.end_stmt()?;
                    continue;
                }
                let (params, body) = self.func_rest()?;
                let kind = match acc.as_deref() {
                    Some("get") => MemberKind::Getter(body),
                    Some(_) => MemberKind::Setter(params, body),
                    None => MemberKind::Method(params, body),
                };
                members.push(Member { name, is_static, kind, line });
            } else {
                let got = self.describe();
                return self.err(format!("Se esperaba «var» o «function» en la clase y se encontró {got}"));
            }
        }
        if !matches!(self.peek(), Tok::Eof) {
            return self.err("Un archivo .as solo puede tener una clase");
        }
        Ok(ClassDef { name, extends, interface, imports, members, line })
    }

    fn var_list(&mut self) -> R<Vec<(String, Option<Expr>)>> {
        let mut v = Vec::new();
        loop {
            let n = self.ident()?;
            self.skip_type()?;
            let init = if self.eat("=") { Some(self.assign()?) } else { None };
            v.push((n, init));
            if !self.eat(",") {
                break;
            }
        }
        Ok(v)
    }

    fn for_stmt(&mut self) -> R<Stmt> {
        self.next();
        self.expect("(")?;
        // for (var k in o) / for (k in o)
        let save = self.i;
        let is_var = self.eat_kw("var");
        if let Tok::Ident(_) = self.peek() {
            let lhs_start = self.i;
            if is_var {
                let n = self.ident()?;
                self.skip_type()?;
                if self.eat_kw("in") {
                    let o = self.expr()?;
                    self.expect(")")?;
                    return Ok(Stmt::ForIn(true, Expr::Ident(n), o, Box::new(self.stmt()?)));
                }
            } else {
                let lhs = self.postfix();
                if let Ok(lhs) = lhs {
                    if self.eat_kw("in") && matches!(lhs, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
                        let o = self.expr()?;
                        self.expect(")")?;
                        return Ok(Stmt::ForIn(false, lhs, o, Box::new(self.stmt()?)));
                    }
                }
                self.i = lhs_start;
            }
        }
        self.i = save;
        let init = if self.is(";") {
            None
        } else if self.eat_kw("var") {
            Some(Box::new(Stmt::Var(self.var_list()?)))
        } else {
            let l = self.line();
            Some(Box::new(Stmt::Expr(self.expr()?, l)))
        };
        self.expect(";")?;
        let cond = if self.is(";") { None } else { Some(self.expr()?) };
        self.expect(";")?;
        let upd = if self.is(")") { None } else { Some(self.expr()?) };
        self.expect(")")?;
        Ok(Stmt::For(init, cond, upd, Box::new(self.stmt()?)))
    }

    fn switch_stmt(&mut self) -> R<Stmt> {
        self.next();
        self.expect("(")?;
        let d = self.expr()?;
        self.expect(")")?;
        self.expect("{")?;
        let mut cases = Vec::new();
        while !self.eat("}") {
            let test = if self.eat_kw("case") {
                let e = self.expr()?;
                Some(e)
            } else if self.eat_kw("default") {
                None
            } else {
                return self.err("Se esperaba «case» o «default»");
            };
            self.expect(":")?;
            let mut body = Vec::new();
            while !self.is_kw("case") && !self.is_kw("default") && !self.is("}") {
                if matches!(self.peek(), Tok::Eof) {
                    return self.err("Falta «}» de cierre del switch");
                }
                body.push(self.stmt()?);
            }
            cases.push((test, body));
        }
        Ok(Stmt::Switch(d, cases))
    }

    fn func_rest(&mut self) -> R<(Vec<String>, Vec<Stmt>)> {
        self.expect("(")?;
        let mut params = Vec::new();
        if !self.is(")") {
            loop {
                params.push(self.ident()?);
                self.skip_type()?;
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        self.skip_type()?;
        Ok((params, self.block()?))
    }

    // ───────── Expressions (from lowest to highest precedence) ─────────

    pub fn expr(&mut self) -> R<Expr> {
        let first = self.assign()?;
        if !self.is(",") {
            return Ok(first);
        }
        let mut v = vec![first];
        while self.eat(",") {
            v.push(self.assign()?);
        }
        Ok(Expr::Comma(v))
    }

    fn assign(&mut self) -> R<Expr> {
        let lhs = self.cond()?;
        let op = match self.peek() {
            Tok::Punct("=") => Some(None),
            Tok::Punct("+=") => Some(Some(BinOp::Add)),
            Tok::Punct("-=") => Some(Some(BinOp::Sub)),
            Tok::Punct("*=") => Some(Some(BinOp::Mul)),
            Tok::Punct("/=") => Some(Some(BinOp::Div)),
            Tok::Punct("%=") => Some(Some(BinOp::Mod)),
            Tok::Punct("&=") => Some(Some(BinOp::BitAnd)),
            Tok::Punct("|=") => Some(Some(BinOp::BitOr)),
            Tok::Punct("^=") => Some(Some(BinOp::BitXor)),
            Tok::Punct("<<=") => Some(Some(BinOp::Shl)),
            Tok::Punct(">>=") => Some(Some(BinOp::Shr)),
            Tok::Punct(">>>=") => Some(Some(BinOp::UShr)),
            _ => None,
        };
        let Some(op) = op else { return Ok(lhs) };
        if !matches!(lhs, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
            return self.err("No se puede asignar a esta expresión");
        }
        self.next();
        let rhs = self.assign()?;
        Ok(Expr::Assign(op, Box::new(lhs), Box::new(rhs)))
    }

    fn cond(&mut self) -> R<Expr> {
        let c = self.or()?;
        if !self.eat("?") {
            return Ok(c);
        }
        let a = self.assign()?;
        self.expect(":")?;
        let b = self.assign()?;
        Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)))
    }

    fn or(&mut self) -> R<Expr> {
        let mut l = self.and()?;
        while self.eat("||") || self.eat_kw("or") {
            l = Expr::Logical(false, Box::new(l), Box::new(self.and()?));
        }
        Ok(l)
    }

    fn and(&mut self) -> R<Expr> {
        let mut l = self.bitor()?;
        while self.eat("&&") || self.eat_kw("and") {
            l = Expr::Logical(true, Box::new(l), Box::new(self.bitor()?));
        }
        Ok(l)
    }

    fn binary(&mut self, ops: &[(&str, BinOp)], next: fn(&mut Self) -> R<Expr>) -> R<Expr> {
        let mut l = next(self)?;
        'outer: loop {
            for (p, op) in ops {
                let hit = if p.chars().all(|c| c.is_alphabetic()) { self.is_kw(p) } else { self.is(p) };
                if hit {
                    self.next();
                    let r = next(self)?;
                    l = Expr::Binary(*op, Box::new(l), Box::new(r));
                    continue 'outer;
                }
            }
            return Ok(l);
        }
    }

    fn bitor(&mut self) -> R<Expr> {
        self.binary(&[("|", BinOp::BitOr)], Self::bitxor)
    }
    fn bitxor(&mut self) -> R<Expr> {
        self.binary(&[("^", BinOp::BitXor)], Self::bitand)
    }
    fn bitand(&mut self) -> R<Expr> {
        self.binary(&[("&", BinOp::BitAnd)], Self::equality)
    }
    fn equality(&mut self) -> R<Expr> {
        self.binary(&[("===", BinOp::StrictEq), ("!==", BinOp::StrictNe), ("==", BinOp::Eq), ("!=", BinOp::Ne)], Self::relational)
    }
    fn relational(&mut self) -> R<Expr> {
        self.binary(&[("<=", BinOp::Le), (">=", BinOp::Ge), ("<", BinOp::Lt), (">", BinOp::Gt), ("instanceof", BinOp::InstanceOf), ("in", BinOp::In)], Self::shift)
    }
    fn shift(&mut self) -> R<Expr> {
        self.binary(&[(">>>", BinOp::UShr), ("<<", BinOp::Shl), (">>", BinOp::Shr)], Self::additive)
    }
    fn additive(&mut self) -> R<Expr> {
        self.binary(&[("+", BinOp::Add), ("-", BinOp::Sub)], Self::multiplicative)
    }
    fn multiplicative(&mut self) -> R<Expr> {
        self.binary(&[("*", BinOp::Mul), ("/", BinOp::Div), ("%", BinOp::Mod)], Self::unary)
    }

    fn unary(&mut self) -> R<Expr> {
        let op = match self.peek() {
            Tok::Punct("!") => Some(UnOp::Not),
            Tok::Punct("-") => Some(UnOp::Neg),
            Tok::Punct("+") => Some(UnOp::Plus),
            Tok::Punct("~") => Some(UnOp::BitNot),
            Tok::Ident(s) if s == "typeof" => Some(UnOp::TypeOf),
            Tok::Ident(s) if s == "void" => Some(UnOp::Void),
            Tok::Ident(s) if s == "not" => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.next();
            let e = self.unary()?;
            return Ok(match (op, e) {
                (UnOp::Neg, Expr::Num(n)) => Expr::Num(-n),
                (op, e) => Expr::Unary(op, Box::new(e)),
            });
        }
        if self.is("++") || self.is("--") {
            let inc = self.is("++");
            self.next();
            let t = self.unary()?;
            if !matches!(t, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
                return self.err("«++» y «--» necesitan una variable o una propiedad");
            }
            return Ok(Expr::Update { inc, prefix: true, target: Box::new(t) });
        }
        if self.eat_kw("delete") {
            let t = self.unary()?;
            return Ok(Expr::Delete(Box::new(t)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> R<Expr> {
        let e = self.call()?;
        if (self.is("++") || self.is("--")) && !self.t[self.i].nl {
            if !matches!(e, Expr::Ident(_) | Expr::Member(..) | Expr::Index(..)) {
                return self.err("«++» y «--» necesitan una variable o una propiedad");
            }
            let inc = self.is("++");
            self.next();
            return Ok(Expr::Update { inc, prefix: false, target: Box::new(e) });
        }
        Ok(e)
    }

    fn args(&mut self) -> R<Vec<Expr>> {
        self.expect("(")?;
        let mut v = Vec::new();
        if !self.is(")") {
            loop {
                v.push(self.assign()?);
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(v)
    }

    fn call(&mut self) -> R<Expr> {
        let mut e = if self.eat_kw("new") {
            // new A.B(…): the constructor without the call, then its arguments.
            let mut c = self.primary()?;
            loop {
                if self.eat(".") {
                    c = Expr::Member(Box::new(c), self.ident()?);
                } else if self.eat("[") {
                    let i = self.expr()?;
                    self.expect("]")?;
                    c = Expr::Index(Box::new(c), Box::new(i));
                } else {
                    break;
                }
            }
            let a = if self.is("(") { self.args()? } else { Vec::new() };
            Expr::New(Box::new(c), a)
        } else {
            self.primary()?
        };
        loop {
            if self.eat(".") {
                e = Expr::Member(Box::new(e), self.ident()?);
            } else if self.eat("[") {
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr::Index(Box::new(e), Box::new(i));
            } else if self.is("(") {
                let a = self.args()?;
                e = Expr::Call(Box::new(e), a);
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> R<Expr> {
        let t = self.next();
        Ok(match t.t {
            Tok::Num(n) => Expr::Num(n),
            Tok::Str(s) => Expr::Str(s),
            Tok::Punct("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                e
            }
            Tok::Punct("[") => {
                let mut v = Vec::new();
                if !self.is("]") {
                    loop {
                        v.push(self.assign()?);
                        if !self.eat(",") || self.is("]") {
                            break;
                        }
                    }
                }
                self.expect("]")?;
                Expr::Array(v)
            }
            Tok::Punct("{") => {
                let mut v = Vec::new();
                if !self.is("}") {
                    loop {
                        let k = match self.next().t {
                            Tok::Ident(s) | Tok::Str(s) => s,
                            Tok::Num(n) => format_num(n),
                            _ => return self.err("Se esperaba el nombre de una propiedad"),
                        };
                        self.expect(":")?;
                        v.push((k, self.assign()?));
                        if !self.eat(",") || self.is("}") {
                            break;
                        }
                    }
                }
                self.expect("}")?;
                Expr::Object(v)
            }
            Tok::Ident(s) => match s.as_str() {
                "true" => Expr::Bool(true),
                "false" => Expr::Bool(false),
                "null" => Expr::Null,
                "undefined" => Expr::Undef,
                "function" => {
                    let name = if let Tok::Ident(_) = self.peek() { Some(self.ident()?) } else { None };
                    let (p, b) = self.func_rest()?;
                    Expr::Func(name, p, b)
                }
                _ if RESERVED.contains(&s.as_str()) => return Err(AsError { line: t.line, msg: format!("«{s}» no está permitido aquí") }),
                _ => Expr::Ident(s),
            },
            Tok::Eof => return Err(AsError { line: t.line, msg: "El script termina antes de tiempo".into() }),
            Tok::Punct(p) => return Err(AsError { line: t.line, msg: format!("«{p}» inesperado") }),
        })
    }
}

pub fn format_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Parses a full script.
pub fn parse(src: &str) -> Result<Vec<Stmt>, AsError> {
    let t = lex(src)?;
    Parser { t, i: 0 }.program()
}

/// Is it a class file (starts with import/class/interface)?
pub fn is_class_file(src: &str) -> bool {
    match lex(src) {
        Ok(t) => t.iter().any(|x| matches!(&x.t, Tok::Ident(s) if s == "class" || s == "interface")) && !t.iter().any(|x| matches!(&x.t, Tok::Ident(s) if s == "on" || s == "onClipEvent")),
        Err(_) => false,
    }
}

/// Parses a class file (.as).
pub fn parse_class(src: &str) -> Result<ClassDef, AsError> {
    let t = lex(src)?;
    Parser { t, i: 0 }.class_file()
}

/// Words for syntax highlighting in the Actions panel.
pub const KEYWORDS: [&str; 44] = [
    "break", "case", "catch", "class", "continue", "default", "delete", "do", "dynamic", "else", "extends", "false", "finally", "for", "function", "get", "if", "implements",
    "import", "in", "instanceof", "interface", "intrinsic", "new", "null", "on", "onClipEvent", "private", "public", "return", "set", "static", "super", "switch", "this",
    "throw", "true", "try", "typeof", "undefined", "var", "void", "while", "with",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typical_flash_scripts() {
        let p = parse("stop();\nvar n:Number = 3, s = 'a';\nfunction f(a:Number, b):Void { return a + b * 2 }\nfor (var i = 0; i < 10; i++) { trace(i) }\nmc.onPress = function () { this._x += 5; };").unwrap();
        assert_eq!(p.len(), 5);
        assert!(matches!(&p[2], Stmt::Func(n, ps, _) if n == "f" && ps.len() == 2));
    }

    #[test]
    fn handlers_and_errors() {
        let p = parse("on (release, keyPress \"<Enter>\") { gotoAndStop(\"fin\") }").unwrap();
        assert!(matches!(&p[0], Stmt::On(ev, _, _) if ev.len() == 2 && ev[1].key.as_deref() == Some("<Enter>")));
        let e = parse("stop(;\n").unwrap_err();
        assert_eq!(e.line, 1);
        let e = parse("x = 1\ny = (2 +\n").unwrap_err();
        assert_eq!(e.line, 3);
        assert!(parse("on (pulsar) {}").is_err());
        assert!(parse("1 = 2").is_err());
    }

    #[test]
    fn try_throw_import_and_classes() {
        let p = parse("import flash.geom.*;\ntry { throw new Error('x') } catch (e:Error) { trace(e) } finally { trace(1) }").unwrap();
        assert!(matches!(&p[0], Stmt::Import(s, 1) if s == "flash.geom.*"));
        assert!(matches!(&p[1], Stmt::Try(_, Some(_), Some(_))));
        assert!(parse("try { }").is_err());
        let c = parse_class("import a.B;\nclass a.Pelota extends B {\n  private var vx:Number = 2;\n  static var n = 0;\n  function Pelota(x) { super(); }\n  public function mover():Void { _x += vx; }\n  function get rapidez() { return vx }\n  function set rapidez(v) { vx = v }\n}").unwrap();
        assert_eq!(c.name, "a.Pelota");
        assert_eq!(c.extends.as_deref(), Some("B"));
        assert_eq!(c.members.len(), 6);
        assert!(c.members[1].is_static);
        assert!(matches!(c.members[4].kind, MemberKind::Getter(_)));
        assert!(parse_class("interface IMovil { function mover():Void; }").unwrap().interface);
        assert!(is_class_file("class A {}") && !is_class_file("trace(1)"));
    }

    #[test]
    fn optional_semicolons_and_for_in() {
        let p = parse("a = 1\nb = 2\nfor (k in o) trace(k)").unwrap();
        assert_eq!(p.len(), 3);
        assert!(matches!(p[2], Stmt::ForIn(false, ..)));
    }
}
