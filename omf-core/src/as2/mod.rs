//! ActionScript 2 → AVM1: the compiler for frame actions, button
//! actions (`on`) and movie clip actions (`onClipEvent` / `on`).
//!
//! The SWF carries the bytecode and Ruffle runs it in "Test Movie".

pub mod class;
pub mod gen;
pub mod parse;

pub use gen::Ctx;
pub use parse::AsError;
use parse::{Event, Expr, Stmt};

/// `#include "file.as"`: replaced by the document's script.
/// Returns the code and, per line, which file and line it comes from.
pub fn preprocess(src: &str, ctx: &Ctx) -> Result<(String, Vec<(Option<String>, usize)>), AsError> {
    let mut out = String::new();
    let mut map = Vec::new();
    fn add(src: &str, file: Option<&str>, ctx: &Ctx, out: &mut String, map: &mut Vec<(Option<String>, usize)>, depth: usize) -> Result<(), AsError> {
        for (i, line) in src.lines().enumerate() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("#include") {
                let name = rest.trim().trim_end_matches(';').trim().trim_matches('"').trim_matches('\'');
                let Some((n, code)) = ctx.scripts.iter().find(|(n, _)| n.eq_ignore_ascii_case(name) || n.rsplit('/').next().map_or(false, |b| b.eq_ignore_ascii_case(name))) else {
                    return Err(AsError { line: i + 1, msg: format!("No se encuentra el archivo de #include «{name}»") });
                };
                if depth > 8 {
                    return Err(AsError { line: i + 1, msg: "#include anidado demasiadas veces".into() });
                }
                add(code, Some(n), ctx, out, map, depth + 1)?;
                continue;
            }
            out.push_str(line);
            out.push('\n');
            map.push((file.map(String::from), i + 1));
        }
        Ok(())
    }
    add(src, None, ctx, &mut out, &mut map, 0)?;
    Ok((out, map))
}

/// Maps the error to the original line (and to the included file, if it comes from one).
fn remap(mut e: AsError, map: &[(Option<String>, usize)]) -> AsError {
    if let Some((file, line)) = map.get(e.line.saturating_sub(1)) {
        match file {
            Some(f) => {
                e.msg = format!("{f}, línea {line}: {}", e.msg);
                // The error line is that of the #include in the main script.
                let inc = map[..e.line - 1].iter().rev().find(|(f, _)| f.is_none()).map_or(1, |(_, l)| l + 1);
                e.line = inc;
            }
            None => e.line = *line,
        }
    }
    e
}

fn parse_with_includes(src: &str, ctx: &Ctx) -> Result<Vec<Stmt>, Vec<AsError>> {
    if !src.contains("#include") {
        return parse::parse(src).map_err(|e| vec![e]);
    }
    let (code, map) = preprocess(src, ctx).map_err(|e| vec![e])?;
    parse::parse(&code).map_err(|e| vec![remap(e, &map)])
}

/// Where a script goes (changes what is allowed).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Frame,
    Button,
    Clip,
}

fn body(src: &str, ctx: &Ctx, stmts: &[Stmt]) -> Result<Vec<u8>, Vec<AsError>> {
    let _ = src;
    let mut g = gen::Gen::new(ctx);
    g.stmts(stmts);
    if !g.errors.is_empty() {
        return Err(g.errors);
    }
    let mut out = g.out;
    out.push(0); // ActionEndFlag
    Ok(out)
}

/// Frame actions (DoAction body, ending in End).
pub fn compile_frame(src: &str, ctx: &Ctx) -> Result<Vec<u8>, Vec<AsError>> {
    let p = parse_with_includes(src, ctx)?;
    body(src, ctx, &p)
}

/// Key code of `keyPress "<Enter>"` / `keyPress "a"`.
fn key_code(k: &str) -> Option<u8> {
    let named = [
        ("<Left>", 1), ("<Right>", 2), ("<Home>", 3), ("<End>", 4), ("<Insert>", 5), ("<Delete>", 6), ("<Backspace>", 8), ("<Enter>", 13), ("<Up>", 14),
        ("<Down>", 15), ("<PageUp>", 16), ("<PageDown>", 17), ("<Tab>", 18), ("<Escape>", 19), ("<Space>", 32),
    ];
    if let Some((_, c)) = named.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)) {
        return Some(*c);
    }
    let mut ch = k.chars();
    match (ch.next(), ch.next()) {
        (Some(c), None) if (c as u32) >= 32 && (c as u32) < 128 => Some(c as u8),
        _ => None,
    }
}

/// BUTTONCONDACTION conditions (16 bits, the first written is bit 15).
fn button_cond(ev: &[Event]) -> Result<u16, String> {
    let mut f = 0u16;
    for e in ev {
        let bit = |i: u32| 1u16 << (15 - i);
        f |= match e.name.as_str() {
            "press" => bit(5),
            "release" => bit(4),
            "releaseOutside" => bit(1),
            "rollOver" => bit(7),
            "rollOut" => bit(6),
            "dragOver" => bit(2) | bit(0),
            "dragOut" => bit(3) | bit(15),
            "keyPress" => {
                let k = e.key.as_deref().and_then(key_code).ok_or_else(|| format!("Tecla no válida: «{}»", e.key.clone().unwrap_or_default()))?;
                ((k & 0x7f) as u16) << 1
            }
            other => return Err(format!("«{other}» no es un evento de botón")),
        };
    }
    Ok(f)
}

/// CLIPACTIONRECORD events (32 bits, the first written is bit 31).
fn clip_flags(ev: &[Event]) -> Result<(u32, Option<u8>), String> {
    let mut f = 0u32;
    let mut key = None;
    for e in ev {
        let bit = |i: u32| 1u32 << (31 - i);
        f |= match e.name.as_str() {
            "keyUp" => bit(0),
            "keyDown" => bit(1),
            "mouseUp" => bit(2),
            "mouseDown" => bit(3),
            "mouseMove" => bit(4),
            "unload" => bit(5),
            "enterFrame" => bit(6),
            "load" => bit(7),
            "dragOver" => bit(8),
            "rollOut" => bit(9),
            "rollOver" => bit(10),
            "releaseOutside" => bit(11),
            "release" => bit(12),
            "press" => bit(13),
            "initialize" => bit(14),
            "data" => bit(15),
            "construct" => bit(21),
            "keyPress" => {
                key = Some(e.key.as_deref().and_then(key_code).ok_or_else(|| "Tecla no válida".to_string())?);
                bit(22)
            }
            "dragOut" => bit(23),
            other => return Err(format!("«{other}» no es un evento de clip")),
        };
    }
    Ok((f, key))
}

/// Actions of a button instance: only `on (…)` handlers.
pub fn compile_button(src: &str, ctx: &Ctx) -> Result<Vec<(u16, Vec<u8>)>, Vec<AsError>> {
    let p = parse_with_includes(src, ctx)?;
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for s in &p {
        match s {
            Stmt::On(ev, b, line) => match button_cond(ev) {
                Ok(c) => match body(src, ctx, b) {
                    Ok(code) => out.push((c, code)),
                    Err(e) => errs.extend(e),
                },
                Err(m) => errs.push(AsError { line: *line, msg: m }),
            },
            Stmt::Empty | Stmt::Import(..) => {}
            Stmt::OnClipEvent(_, _, line) => errs.push(AsError { line: *line, msg: "onClipEvent solo se puede usar en clips de película".into() }),
            _ => errs.push(AsError { line: stmt_line(s), msg: "Las acciones de un botón deben ir dentro de un controlador on (por ejemplo on (release) { … })".into() }),
        }
    }
    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

/// Actions of a movie clip instance: `onClipEvent (…)` and `on (…)`.
pub fn compile_clip(src: &str, ctx: &Ctx) -> Result<Vec<(u32, Option<u8>, Vec<u8>)>, Vec<AsError>> {
    let p = parse_with_includes(src, ctx)?;
    let mut out = Vec::new();
    let mut errs = Vec::new();
    for s in &p {
        match s {
            Stmt::On(ev, b, line) | Stmt::OnClipEvent(ev, b, line) => match clip_flags(ev) {
                Ok((f, key)) => match body(src, ctx, b) {
                    Ok(code) => out.push((f, key, code)),
                    Err(e) => errs.extend(e),
                },
                Err(m) => errs.push(AsError { line: *line, msg: m }),
            },
            Stmt::Empty | Stmt::Import(..) => {}
            _ => errs.push(AsError { line: stmt_line(s), msg: "Las acciones de un clip deben ir dentro de onClipEvent (…) o on (…)".into() }),
        }
    }
    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

fn stmt_line(s: &Stmt) -> usize {
    match s {
        Stmt::Expr(_, l) | Stmt::Break(l) | Stmt::Continue(l) | Stmt::On(_, _, l) | Stmt::OnClipEvent(_, _, l) | Stmt::Throw(_, l) | Stmt::Import(_, l) => *l,
        _ => 1,
    }
}

/// Checks the syntax (Check Syntax button in the Actions panel).
pub fn check(src: &str, kind: Kind, ctx: &Ctx) -> Vec<AsError> {
    match kind {
        Kind::Frame => compile_frame(src, ctx).err(),
        Kind::Button => compile_button(src, ctx).err(),
        Kind::Clip => compile_clip(src, ctx).err(),
    }
    .unwrap_or_default()
}

/// "Enable Simple Frame Actions": what the editor obeys during playback.
#[derive(Clone, Debug, PartialEq)]
pub enum Simple {
    Stop,
    Play,
    Next,
    Prev,
    /// (frame from 1 or label, play)
    Goto(Result<u32, String>, bool),
}

pub fn simple_actions(src: &str) -> Vec<Simple> {
    let Ok(p) = parse::parse(src) else { return Vec::new() };
    let mut v = Vec::new();
    for s in p {
        let Stmt::Expr(Expr::Call(f, a), _) = s else { continue };
        let Expr::Ident(n) = *f else { continue };
        match (n.as_str(), a.as_slice()) {
            ("stop", []) => v.push(Simple::Stop),
            ("play", []) => v.push(Simple::Play),
            ("nextFrame", []) => v.push(Simple::Next),
            ("prevFrame", []) => v.push(Simple::Prev),
            ("gotoAndPlay" | "gotoAndStop", [x]) => {
                let target = match x {
                    Expr::Num(f) if *f >= 1.0 => Ok(*f as u32),
                    Expr::Str(l) => Err(l.clone()),
                    _ => continue,
                };
                v.push(Simple::Goto(target, n == "gotoAndPlay"));
            }
            _ => {}
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(s: &str) -> Vec<u8> {
        compile_frame(s, &Ctx::default()).unwrap_or_else(|e| panic!("{e:?}"))
    }

    #[test]
    fn basic_actions() {
        assert_eq!(frame("stop();"), vec![0x07, 0x00]);
        assert_eq!(frame("gotoAndPlay(5)"), vec![0x81, 2, 0, 4, 0, 0x06, 0x00]);
        // trace("hi"): Push "hi" + Trace
        assert_eq!(frame("trace('hi')"), vec![0x96, 4, 0, 0, b'h', b'i', 0, 0x26, 0x00]);
        // gotoAndStop("fin"): GoToLabel + Stop
        assert_eq!(frame("gotoAndStop(\"fin\")"), vec![0x8C, 4, 0, b'f', b'i', b'n', 0, 0x07, 0x00]);
    }

    #[test]
    fn numbers_and_variables() {
        // x = 1.5 → Push "x", Push double(1.5), SetVariable
        let b = frame("x = 1.5");
        assert_eq!(&b[..6], &[0x96, 3, 0, 0, b'x', 0]);
        assert_eq!(b[6..9], [0x96, 9, 0]);
        assert_eq!(b[9], 6);
        let d = 1.5f64.to_le_bytes();
        assert_eq!(&b[10..14], &d[4..8]);
        assert_eq!(&b[14..18], &d[0..4]);
        assert_eq!(&b[18..], &[0x1D, 0x00]);
    }

    #[test]
    fn control_flow_compiles() {
        let src = r#"
            var total = 0;
            for (var i = 0; i < 10; i++) { if (i == 5) continue; if (i > 8) break; total += i; }
            var o = {a: 1, b: [1, 2, 3]};
            for (var k in o) { trace(k); if (k == "b") break; }
            switch (total) { case 1: trace("uno"); break; default: trace("otro"); }
            function suma(a, b) { return a + b; }
            var f = function (x) { return x * 2; };
            trace(suma(1, 2) + f(3) + Math.max(1, 2));
            do { total--; } while (total > 0 && !false);
            _root.mc._x += 10; mc.onPress = function () { this._alpha = 50; };
        "#;
        let b = frame(src);
        assert_eq!(*b.last().unwrap(), 0);
        // Each long action can be walked by its length up to the End.
        let mut i = 0;
        while b[i] != 0 {
            i += if b[i] >= 0x80 { 3 + u16::from_le_bytes([b[i + 1], b[i + 2]]) as usize } else { 1 };
        }
        assert_eq!(i, b.len() - 1);
    }

    #[test]
    fn errors_have_lines() {
        let e = compile_frame("stop()\nbreak;", &Ctx::default()).unwrap_err();
        assert_eq!(e[0].line, 2);
        assert!(compile_frame("on (release) { play() }", &Ctx::default()).is_err());
        assert!(compile_button("stop();", &Ctx::default()).is_err());
    }

    #[test]
    fn try_catch_and_includes() {
        let b = frame("try { throw 'x'; } catch (e) { trace(e); } finally { stop(); }");
        let i = b.iter().position(|c| *c == 0x8F).unwrap();
        assert_eq!(b[i + 3], 3, "catch y finally");
        let ctx = Ctx { scripts: vec![("util.as".into(), "function doble(x) { return x * 2 }".into())], ..Default::default() };
        assert!(compile_frame("#include \"util.as\"\ntrace(doble(2))", &ctx).is_ok());
        let e = compile_frame("#include \"nada.as\"", &ctx).unwrap_err();
        assert!(e[0].msg.contains("nada.as"));
        let ctx = Ctx { scripts: vec![("mal.as".into(), "x = 1\ny = )".into())], ..Default::default() };
        let e = compile_frame("stop()\n#include \"mal.as\"\nplay()", &ctx).unwrap_err();
        assert_eq!(e[0].line, 2);
        assert!(e[0].msg.starts_with("mal.as, línea 2"), "{}", e[0].msg);
    }

    #[test]
    fn handlers() {
        let b = compile_button("on (release) { gotoAndStop(2); }\non (keyPress \"<Enter>\") { play(); }", &Ctx::default()).unwrap();
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].0, 1 << 11, "OverDownToOverUp");
        assert_eq!(b[1].0, 13 << 1);
        let c = compile_clip("onClipEvent (enterFrame) { _x += 1; }\non (press) { startDrag(this); }", &Ctx::default()).unwrap();
        assert_eq!(c[0].0, 1 << 25);
        assert_eq!(c[1].0, 1 << 18);
    }

    #[test]
    fn scenes_and_simple_actions() {
        let ctx = Ctx { scenes: vec![("Escena 1".into(), 0), ("Escena 2".into(), 10)], ..Default::default() };
        let b = compile_frame("gotoAndPlay(\"Escena 2\", 3)", &ctx).unwrap();
        assert_eq!(&b[..5], &[0x81, 2, 0, 12, 0]);
        assert_eq!(
            simple_actions("stop(); gotoAndPlay(\"intro\"); gotoAndStop(4); x = 1"),
            vec![Simple::Stop, Simple::Goto(Err("intro".into()), true), Simple::Goto(Ok(4), false)]
        );
    }
}
