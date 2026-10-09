//! ActionScript 2 classes (the document's .as files).
//!
//! Each class is translated to what Flash generated: the constructor in
//! `_global.package.Class`, `ActionExtends` with the superclass, methods and
//! initialized variables on the prototype, static members on the class
//! itself and `get`/`set` via `addProperty`. Inside methods, member names
//! are qualified with `this.` (or with the class, if static), as the
//! Flash compiler does. Everything goes into the init actions of the first
//! frame, superclasses first, followed by the `Object.registerClass` calls
//! for symbols with a linkage class.

use super::gen::Gen;
use super::parse::*;
use super::{AsError, Ctx};
use std::collections::{HashMap, HashSet};

/// `_global.a.b.C` as an expression.
fn global_path(full: &str) -> Expr {
    full.split('.').fold(Expr::Ident("_global".into()), |e, k| Expr::Member(Box::new(e), k.into()))
}

/// `a.b.C` as is (for classes not in the document: MovieClip, mx.…).
fn dotted(full: &str) -> Expr {
    let mut it = full.split('.');
    let first = Expr::Ident(it.next().unwrap_or("Object").into());
    it.fold(first, |e, k| Expr::Member(Box::new(e), k.into()))
}

fn member(o: Expr, k: &str) -> Expr {
    Expr::Member(Box::new(o), k.into())
}

fn assign(t: Expr, v: Expr) -> Stmt {
    Stmt::Expr(Expr::Assign(None, Box::new(t), Box::new(v)), 0)
}

/// Fully qualified name of `short` as seen from a class (imports and its package).
fn resolve(short: &str, def: &ClassDef, known: &[String]) -> String {
    if short.contains('.') {
        return short.to_string();
    }
    for imp in &def.imports {
        if imp.rsplit('.').next() == Some(short) && !imp.ends_with(".*") {
            return imp.clone();
        }
        if let Some(pkg) = imp.strip_suffix(".*") {
            let cand = format!("{pkg}.{short}");
            if known.contains(&cand) {
                return cand;
            }
        }
    }
    if let Some((pkg, _)) = def.name.rsplit_once('.') {
        let cand = format!("{pkg}.{short}");
        if known.contains(&cand) {
            return cand;
        }
    }
    short.to_string()
}

// ───────── Qualifying members with this. / Class. ─────────

struct Scope<'a> {
    inst: &'a HashSet<String>,
    stat: &'a HashMap<String, String>,
    /// In a static method there are no instance members.
    is_static: bool,
}

fn vars_of(v: &[Stmt], out: &mut HashSet<String>) {
    for s in v {
        vars_of_stmt(s, out);
    }
}

fn vars_of_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Var(v) => out.extend(v.iter().map(|(n, _)| n.clone())),
        Stmt::Func(n, ..) => {
            out.insert(n.clone());
        }
        Stmt::Block(v) => vars_of(v, out),
        Stmt::If(_, t, e) => {
            vars_of_stmt(t, out);
            if let Some(e) = e {
                vars_of_stmt(e, out);
            }
        }
        Stmt::While(_, b) | Stmt::DoWhile(b, _) | Stmt::With(_, b) => vars_of_stmt(b, out),
        Stmt::For(i, _, _, b) => {
            if let Some(i) = i {
                vars_of_stmt(i, out);
            }
            vars_of_stmt(b, out);
        }
        Stmt::ForIn(is_var, t, _, b) => {
            if let (true, Expr::Ident(n)) = (is_var, t) {
                out.insert(n.clone());
            }
            vars_of_stmt(b, out);
        }
        Stmt::Switch(_, cases) => {
            for (_, b) in cases {
                vars_of(b, out);
            }
        }
        Stmt::Try(b, c, f) => {
            vars_of(b, out);
            if let Some((n, c)) = c {
                out.insert(n.clone());
                vars_of(c, out);
            }
            if let Some(f) = f {
                vars_of(f, out);
            }
        }
        _ => {}
    }
}

fn q_body(sc: &Scope, outer: &HashSet<String>, params: &[String], body: &mut [Stmt]) {
    let mut locals = outer.clone();
    locals.extend(params.iter().cloned());
    vars_of(body, &mut locals);
    for s in body.iter_mut() {
        q_stmt(sc, &locals, s);
    }
}

fn q_stmt(sc: &Scope, l: &HashSet<String>, s: &mut Stmt) {
    match s {
        Stmt::Expr(e, _) | Stmt::Throw(e, _) => q_expr(sc, l, e),
        Stmt::Var(v) => {
            for (_, e) in v.iter_mut() {
                if let Some(e) = e {
                    q_expr(sc, l, e);
                }
            }
        }
        Stmt::If(c, t, e) => {
            q_expr(sc, l, c);
            q_stmt(sc, l, t);
            if let Some(e) = e {
                q_stmt(sc, l, e);
            }
        }
        Stmt::While(c, b) | Stmt::DoWhile(b, c) | Stmt::With(c, b) => {
            q_expr(sc, l, c);
            q_stmt(sc, l, b);
        }
        Stmt::For(i, c, u, b) => {
            if let Some(i) = i {
                q_stmt(sc, l, i);
            }
            if let Some(c) = c {
                q_expr(sc, l, c);
            }
            if let Some(u) = u {
                q_expr(sc, l, u);
            }
            q_stmt(sc, l, b);
        }
        Stmt::ForIn(is_var, t, o, b) => {
            if !*is_var {
                q_expr(sc, l, t);
            }
            q_expr(sc, l, o);
            q_stmt(sc, l, b);
        }
        Stmt::Block(v) => {
            for s in v {
                q_stmt(sc, l, s);
            }
        }
        Stmt::Return(e) => {
            if let Some(e) = e {
                q_expr(sc, l, e);
            }
        }
        Stmt::Switch(d, cases) => {
            q_expr(sc, l, d);
            for (t, b) in cases {
                if let Some(t) = t {
                    q_expr(sc, l, t);
                }
                for s in b {
                    q_stmt(sc, l, s);
                }
            }
        }
        Stmt::Func(_, p, b) => q_body(sc, l, p, b),
        Stmt::Try(b, c, f) => {
            for s in b {
                q_stmt(sc, l, s);
            }
            if let Some((_, c)) = c {
                for s in c {
                    q_stmt(sc, l, s);
                }
            }
            if let Some(f) = f {
                for s in f {
                    q_stmt(sc, l, s);
                }
            }
        }
        _ => {}
    }
}

fn q_expr(sc: &Scope, l: &HashSet<String>, e: &mut Expr) {
    match e {
        Expr::Ident(n) if !l.contains(n.as_str()) => {
            if !sc.is_static && sc.inst.contains(n.as_str()) {
                *e = member(Expr::Ident("this".into()), n);
            } else if let Some(owner) = sc.stat.get(n.as_str()) {
                *e = member(global_path(owner), n);
            }
        }
        Expr::Member(o, _) => q_expr(sc, l, o),
        Expr::Index(o, i) => {
            q_expr(sc, l, o);
            q_expr(sc, l, i);
        }
        Expr::Call(f, a) | Expr::New(f, a) => {
            q_expr(sc, l, f);
            for x in a {
                q_expr(sc, l, x);
            }
        }
        Expr::Unary(_, x) | Expr::Delete(x) => q_expr(sc, l, x),
        Expr::Update { target, .. } => q_expr(sc, l, target),
        Expr::Binary(_, a, b) | Expr::Logical(_, a, b) | Expr::Assign(_, a, b) => {
            q_expr(sc, l, a);
            q_expr(sc, l, b);
        }
        Expr::Cond(c, a, b) => {
            q_expr(sc, l, c);
            q_expr(sc, l, a);
            q_expr(sc, l, b);
        }
        Expr::Array(v) | Expr::Comma(v) => {
            for x in v {
                q_expr(sc, l, x);
            }
        }
        Expr::Object(v) => {
            for (_, x) in v {
                q_expr(sc, l, x);
            }
        }
        Expr::Func(_, p, b) => q_body(sc, l, p, b),
        _ => {}
    }
}

fn calls_super(v: &[Stmt]) -> bool {
    v.iter().any(|s| matches!(s, Stmt::Expr(Expr::Call(f, _), _) if matches!(&**f, Expr::Ident(n) if n == "super")))
}

/// Already parsed document classes: (file, definition) and errors.
pub fn parse_all(ctx: &Ctx) -> (Vec<(String, ClassDef)>, Vec<(String, AsError)>) {
    let mut defs = Vec::new();
    let mut errs = Vec::new();
    for (file, src) in &ctx.scripts {
        if !is_class_file(src) {
            continue;
        }
        match parse_class(src) {
            Ok(d) => {
                // The file must be named like the class (Flash requires it).
                let short = d.name.rsplit('.').next().unwrap_or("");
                let base = file.rsplit('/').next().unwrap_or(file).trim_end_matches(".as");
                if !base.eq_ignore_ascii_case(short) {
                    errs.push((file.clone(), AsError { line: d.line, msg: format!("La clase «{}» debe estar en un archivo llamado {short}.as", d.name) }));
                    continue;
                }
                defs.push((file.clone(), d));
            }
            Err(e) => errs.push((file.clone(), e)),
        }
    }
    (defs, errs)
}

/// Fully qualified names of the scripts' classes (for `import a.*`).
pub fn class_names(scripts: &[(String, String)]) -> Vec<String> {
    let ctx = Ctx { scripts: scripts.to_vec(), ..Default::default() };
    parse_all(&ctx).0.into_iter().map(|(_, d)| d.name).collect()
}

/// Compiles the document's classes and the registrations of linkage
/// classes (identifier, class). Returns the actions (ending in End,
/// empty if there is nothing) and the errors with their file.
pub fn compile_classes(ctx: &Ctx, register: &[(String, String)]) -> (Vec<u8>, Vec<(String, AsError)>) {
    let (defs, mut errs) = parse_all(ctx);
    let known: Vec<String> = defs.iter().map(|(_, d)| d.name.clone()).collect();
    let by_name: HashMap<String, usize> = defs.iter().enumerate().map(|(i, (_, d))| (d.name.clone(), i)).collect();
    // Superclasses first.
    let mut order = Vec::new();
    let mut state = vec![0u8; defs.len()];
    fn visit(i: usize, defs: &[(String, ClassDef)], by: &HashMap<String, usize>, known: &[String], st: &mut [u8], order: &mut Vec<usize>, errs: &mut Vec<(String, AsError)>) {
        if st[i] == 2 {
            return;
        }
        if st[i] == 1 {
            errs.push((defs[i].0.clone(), AsError { line: defs[i].1.line, msg: format!("La clase «{}» se hereda a sí misma", defs[i].1.name) }));
            return;
        }
        st[i] = 1;
        if let Some(sup) = &defs[i].1.extends {
            if let Some(&j) = by.get(&resolve(sup, &defs[i].1, known)) {
                visit(j, defs, by, known, st, order, errs);
            }
        }
        st[i] = 2;
        order.push(i);
    }
    for i in 0..defs.len() {
        visit(i, &defs, &by_name, &known, &mut state, &mut order, &mut errs);
    }

    let mut out = Vec::new();
    let ctx2 = Ctx { classes: known.clone(), ..ctx.clone() };
    for i in order {
        let (file, def) = &defs[i];
        if def.interface {
            continue;
        }
        // Own and inherited members (from the document's classes).
        let mut inst = HashSet::new();
        let mut stat: HashMap<String, String> = HashMap::new();
        let mut cur = Some(i);
        let mut guard = 0;
        while let Some(k) = cur {
            let d = &defs[k].1;
            for m in &d.members {
                if m.is_static {
                    stat.entry(m.name.clone()).or_insert_with(|| d.name.clone());
                } else {
                    inst.insert(m.name.clone());
                }
            }
            cur = d.extends.as_ref().and_then(|s| by_name.get(&resolve(s, d, &known)).copied());
            guard += 1;
            if guard > 64 {
                break;
            }
        }
        let short = def.name.rsplit('.').next().unwrap_or(&def.name).to_string();
        inst.remove(&short);
        let full = &def.name;
        let mut stmts = Vec::new();
        // Packages: _global.a, _global.a.b…
        let parts: Vec<&str> = full.split('.').collect();
        for n in 1..parts.len() {
            let p = global_path(&parts[..n].join("."));
            stmts.push(Stmt::If(Expr::Unary(UnOp::Not, Box::new(p.clone())), Box::new(assign(p, Expr::Object(Vec::new()))), None));
        }
        let mut inner = Vec::new();
        let ctor = def.members.iter().find(|m| m.name == short && !m.is_static && matches!(m.kind, MemberKind::Method(..)));
        let (params, mut body) = match ctor.map(|m| &m.kind) {
            Some(MemberKind::Method(p, b)) => (p.clone(), b.clone()),
            _ => (Vec::new(), Vec::new()),
        };
        let sc = Scope { inst: &inst, stat: &stat, is_static: false };
        q_body(&sc, &HashSet::new(), &params, &mut body);
        if def.extends.is_some() && !calls_super(&body) {
            body.insert(0, Stmt::Expr(Expr::Call(Box::new(Expr::Ident("super".into())), Vec::new()), 0));
        }
        inner.push(assign(global_path(full), Expr::Func(None, params, body)));
        if let Some(sup) = &def.extends {
            let s = resolve(sup, def, &known);
            let sup_e = if known.contains(&s) { global_path(&s) } else { dotted(&s) };
            inner.push(Stmt::Extends(global_path(full), sup_e));
        }
        let proto = member(global_path(full), "prototype");
        let mut accessors: Vec<(String, bool, bool, bool)> = Vec::new(); // name, static, get, set
        for m in &def.members {
            if ctor.map_or(false, |c| std::ptr::eq(c, m)) {
                continue;
            }
            let target = if m.is_static { global_path(full) } else { proto.clone() };
            let sc = Scope { inst: &inst, stat: &stat, is_static: m.is_static };
            match &m.kind {
                MemberKind::Var(Some(init)) => {
                    let mut init = init.clone();
                    q_expr(&sc, &HashSet::new(), &mut init);
                    inner.push(Stmt::Expr(Expr::Assign(None, Box::new(member(target, &m.name)), Box::new(init)), m.line));
                }
                MemberKind::Var(None) => {}
                MemberKind::Method(p, b) => {
                    let mut b = b.clone();
                    q_body(&sc, &HashSet::new(), p, &mut b);
                    inner.push(assign(member(target, &m.name), Expr::Func(None, p.clone(), b)));
                }
                MemberKind::Getter(b) | MemberKind::Setter(_, b) => {
                    let is_get = matches!(m.kind, MemberKind::Getter(_));
                    let p = match &m.kind {
                        MemberKind::Setter(p, _) => p.clone(),
                        _ => Vec::new(),
                    };
                    let mut b = b.clone();
                    q_body(&sc, &HashSet::new(), &p, &mut b);
                    let key = format!("__{}__{}", if is_get { "get" } else { "set" }, m.name);
                    inner.push(assign(member(target, &key), Expr::Func(None, p, b)));
                    match accessors.iter_mut().find(|a| a.0 == m.name && a.1 == m.is_static) {
                        Some(a) => {
                            a.2 |= is_get;
                            a.3 |= !is_get;
                        }
                        None => accessors.push((m.name.clone(), m.is_static, is_get, !is_get)),
                    }
                }
            }
        }
        for (name, is_static, g, s) in accessors {
            let target = if is_static { global_path(full) } else { proto.clone() };
            let getter = if g { member(target.clone(), &format!("__get__{name}")) } else { Expr::Func(None, Vec::new(), Vec::new()) };
            let setter = if s { member(target.clone(), &format!("__set__{name}")) } else { Expr::Null };
            inner.push(Stmt::Expr(Expr::Call(Box::new(member(target, "addProperty")), vec![Expr::Str(name), getter, setter]), 0));
        }
        // Prototype members do not show up in for…in.
        inner.push(Stmt::Expr(Expr::Call(Box::new(Expr::Ident("ASSetPropFlags".into())), vec![proto, Expr::Null, Expr::Num(1.0)]), 0));
        stmts.push(Stmt::If(Expr::Unary(UnOp::Not, Box::new(global_path(full))), Box::new(Stmt::Block(inner)), None));

        let mut g = Gen::new(&ctx2);
        g.fn2 = true;
        for imp in &def.imports {
            g.import(imp);
        }
        // Classes in the same package are visible by their short name.
        if let Some((pkg, _)) = full.rsplit_once('.') {
            g.import(&format!("{pkg}.*"));
        }
        g.stmts(&stmts);
        if g.errors.is_empty() {
            out.extend_from_slice(&g.out);
        } else {
            errs.extend(g.errors.into_iter().map(|e| (file.clone(), e)));
        }
    }
    // Object.registerClass("linkage", Class)
    if !register.is_empty() {
        let mut g = Gen::new(&ctx2);
        let mut v = Vec::new();
        for (id, class) in register {
            let full = if known.contains(class) { class.clone() } else { known.iter().find(|k| k.rsplit('.').next() == Some(class.as_str())).cloned().unwrap_or_else(|| class.clone()) };
            let c = if known.contains(&full) { global_path(&full) } else { dotted(&full) };
            v.push(Stmt::Expr(Expr::Call(Box::new(member(Expr::Ident("Object".into()), "registerClass")), vec![Expr::Str(id.clone()), c]), 0));
        }
        g.stmts(&v);
        out.extend_from_slice(&g.out);
    }
    if !out.is_empty() {
        out.push(0);
    }
    (out, errs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn walk(b: &[u8]) -> usize {
        let mut i = 0;
        while i < b.len() && b[i] != 0 {
            i += if b[i] >= 0x80 { 3 + u16::from_le_bytes([b[i + 1], b[i + 2]]) as usize } else { 1 };
        }
        i
    }

    #[test]
    fn classes_compile_in_order_with_members() {
        let ctx = Ctx {
            scripts: vec![
                ("juego/Pelota.as".into(), "import juego.Cosa;\nclass juego.Pelota extends Cosa {\n var vx = 3;\n static var total = 0;\n function Pelota(x) { total++; this.x = x; }\n function mover() { x += vx; }\n function get rapidez() { return vx }\n}".into()),
                ("juego/Cosa.as".into(), "class juego.Cosa { var x = 0; function Cosa() {} }".into()),
                ("util.as".into(), "function f() {}".into()),
            ],
            ..Default::default()
        };
        let (code, errs) = compile_classes(&ctx, &[("pelota".into(), "Pelota".into())]);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(walk(&code), code.len() - 1, "acciones bien formadas");
        let s = String::from_utf8_lossy(&code);
        // Cosa is defined before Pelota; ActionExtends and DefineFunction2.
        assert!(s.find("Cosa").unwrap() < s.find("Pelota").unwrap());
        assert!(code.contains(&0x69) && code.contains(&0x8E));
        assert!(s.contains("__get__rapidez") && s.contains("addProperty") && s.contains("registerClass") && s.contains("pelota"));
        assert_eq!(class_names(&ctx.scripts).len(), 2);
    }

    #[test]
    fn member_names_get_this() {
        let inst: HashSet<String> = ["vx".to_string()].into();
        let stat: HashMap<String, String> = [("total".to_string(), "P".to_string())].into();
        let sc = Scope { inst: &inst, stat: &stat, is_static: false };
        let mut b = parse("var vy = 1; vx += vy; total++; function g(vx) { return vx; }").unwrap();
        q_body(&sc, &HashSet::new(), &[], &mut b);
        assert!(matches!(&b[1], Stmt::Expr(Expr::Assign(_, t, _), _) if matches!(&**t, Expr::Member(o, k) if k == "vx" && matches!(&**o, Expr::Ident(t) if t == "this"))));
        assert!(matches!(&b[2], Stmt::Expr(Expr::Update { target, .. }, _) if matches!(&**target, Expr::Member(_, k) if k == "total")));
        // The parameter shadows the member.
        let Stmt::Func(_, _, body) = &b[3] else { panic!() };
        assert!(matches!(&body[0], Stmt::Return(Some(Expr::Ident(n))) if n == "vx"));
    }

    #[test]
    fn wrong_file_name_is_an_error() {
        let ctx = Ctx { scripts: vec![("Otra.as".into(), "class Pelota {}".into())], ..Default::default() };
        let (_, errs) = compile_classes(&ctx, &[]);
        assert!(errs[0].1.msg.contains("Pelota.as"));
    }
}
