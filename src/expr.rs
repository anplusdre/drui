// A small expression language for prototypes: the condition of an interaction ("score >= 3 && !done") and the value a Set variable
// action writes ("count + 1"). Numbers, strings in quotes, true / false, variable names, + - * / %, comparisons, && || !, and
// parentheses. A variable is found by its name ("count") or by "Collection / name".
#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Num(f64),
    Bool(bool),
    Str(String),
}

impl Val {
    pub fn truthy(&self) -> bool {
        match self {
            Val::Bool(b) => *b,
            Val::Num(n) => *n != 0.0,
            Val::Str(s) => !s.is_empty(),
        }
    }

    // how the value reads when it is written into a text, number or boolean variable
    pub fn text(&self) -> String {
        match self {
            Val::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
            Val::Num(n) => format!("{n}"),
            Val::Bool(b) => b.to_string(),
            Val::Str(s) => s.clone(),
        }
    }

    fn num(&self) -> Result<f64, String> {
        match self {
            Val::Num(n) => Ok(*n),
            Val::Bool(b) => Ok(f64::from(*b)),
            Val::Str(s) => s.trim().parse().map_err(|_| format!("\"{s}\" is not a number")),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Name(String),
    Op(&'static str),
}

fn tokens(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let s = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(c[s..i].iter().collect::<String>().parse().map_err(|_| "a number is not valid".to_owned())?));
        } else if ch == '"' || ch == '\'' {
            let s = i + 1;
            i += 1;
            while i < c.len() && c[i] != ch {
                i += 1;
            }
            if i >= c.len() {
                return Err("a quote is not closed".into());
            }
            out.push(Tok::Str(c[s..i].iter().collect()));
            i += 1;
        } else if ch.is_alphabetic() || ch == '_' {
            // a name: letters, digits, spaces inside "Collection / name" are not allowed unless the name is quoted with backticks
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_' || c[i] == '.') {
                i += 1;
            }
            out.push(Tok::Name(c[s..i].iter().collect()));
        } else if ch == '`' {
            let s = i + 1;
            i += 1;
            while i < c.len() && c[i] != '`' {
                i += 1;
            }
            out.push(Tok::Name(c[s..i.min(c.len())].iter().collect()));
            i += 1;
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            let op = ["==", "!=", "<=", ">=", "&&", "||"].iter().find(|o| **o == two).copied();
            match op {
                Some(o) => {
                    out.push(Tok::Op(o));
                    i += 2;
                }
                None => {
                    let one = ["+", "-", "*", "/", "%", "<", ">", "!", "(", ")"].iter().find(|o| o.starts_with(ch)).copied().ok_or_else(|| format!("'{ch}' is not understood"))?;
                    out.push(Tok::Op(one));
                    i += 1;
                }
            }
        }
    }
    Ok(out)
}

struct P<'a> {
    t: Vec<Tok>,
    at: usize,
    vars: &'a dyn Fn(&str) -> Option<Val>,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.at)
    }
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn or(&mut self) -> Result<Val, String> {
        let mut l = self.and()?;
        while self.eat("||") {
            let r = self.and()?;
            l = Val::Bool(l.truthy() || r.truthy());
        }
        Ok(l)
    }
    fn and(&mut self) -> Result<Val, String> {
        let mut l = self.cmp()?;
        while self.eat("&&") {
            let r = self.cmp()?;
            l = Val::Bool(l.truthy() && r.truthy());
        }
        Ok(l)
    }
    fn cmp(&mut self) -> Result<Val, String> {
        let l = self.sum()?;
        for op in ["==", "!=", "<=", ">=", "<", ">"] {
            if self.eat(op) {
                let r = self.sum()?;
                let ord = match (&l, &r) {
                    (Val::Str(a), Val::Str(b)) => a.cmp(b),
                    _ => l.num()?.partial_cmp(&r.num()?).ok_or("numbers cannot be compared")?,
                };
                let eq = match (&l, &r) {
                    (Val::Str(a), Val::Str(b)) => a == b,
                    (Val::Bool(a), Val::Bool(b)) => a == b,
                    _ => ord == std::cmp::Ordering::Equal,
                };
                return Ok(Val::Bool(match op {
                    "==" => eq,
                    "!=" => !eq,
                    "<=" => ord != std::cmp::Ordering::Greater,
                    ">=" => ord != std::cmp::Ordering::Less,
                    "<" => ord == std::cmp::Ordering::Less,
                    _ => ord == std::cmp::Ordering::Greater,
                }));
            }
        }
        Ok(l)
    }
    fn sum(&mut self) -> Result<Val, String> {
        let mut l = self.prod()?;
        loop {
            if self.eat("+") {
                let r = self.prod()?;
                l = match (&l, &r) {
                    (Val::Str(a), _) => Val::Str(format!("{a}{}", r.text())),
                    (_, Val::Str(b)) => Val::Str(format!("{}{b}", l.text())),
                    _ => Val::Num(l.num()? + r.num()?),
                };
            } else if self.eat("-") {
                let r = self.prod()?;
                l = Val::Num(l.num()? - r.num()?);
            } else {
                return Ok(l);
            }
        }
    }
    fn prod(&mut self) -> Result<Val, String> {
        let mut l = self.unary()?;
        loop {
            let op = ["*", "/", "%"].into_iter().find(|o| self.eat(o));
            let Some(op) = op else { return Ok(l) };
            let (a, b) = (l.num()?, self.unary()?.num()?);
            l = Val::Num(match op {
                "*" => a * b,
                "/" if b == 0.0 => return Err("division by zero".into()),
                "/" => a / b,
                _ if b == 0.0 => return Err("division by zero".into()),
                _ => a % b,
            });
        }
    }
    fn unary(&mut self) -> Result<Val, String> {
        if self.eat("!") {
            return Ok(Val::Bool(!self.unary()?.truthy()));
        }
        if self.eat("-") {
            return Ok(Val::Num(-self.unary()?.num()?));
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Val, String> {
        match self.t.get(self.at).cloned() {
            Some(Tok::Num(n)) => {
                self.at += 1;
                Ok(Val::Num(n))
            }
            Some(Tok::Str(s)) => {
                self.at += 1;
                Ok(Val::Str(s))
            }
            Some(Tok::Name(n)) => {
                self.at += 1;
                match n.as_str() {
                    "true" => Ok(Val::Bool(true)),
                    "false" => Ok(Val::Bool(false)),
                    _ => (self.vars)(&n).ok_or_else(|| format!("no variable called {n}")),
                }
            }
            Some(Tok::Op("(")) => {
                self.at += 1;
                let v = self.or()?;
                if !self.eat(")") {
                    return Err("a ) is missing".into());
                }
                Ok(v)
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("the expression ends too early".into()),
        }
    }
}

// evaluates `src`; `vars` finds a variable by name
pub fn eval(src: &str, vars: &dyn Fn(&str) -> Option<Val>) -> Result<Val, String> {
    let mut p = P { t: tokens(src)?, at: 0, vars };
    let v = p.or()?;
    if p.at < p.t.len() {
        return Err("something is left over after the expression".into());
    }
    Ok(v)
}

// ---- the expression as a tree, for the code exports (`proto_plan.rs` writes it as JavaScript, Kotlin or Dart)

#[derive(Clone, Debug, PartialEq)]
pub enum Ex {
    Num(f64),
    Str(String),
    Bool(bool),
    Var(usize), // an index the caller's lookup gave
    Not(Box<Ex>),
    Neg(Box<Ex>),
    Bin(&'static str, Box<Ex>, Box<Ex>),
}

struct T<'a> {
    t: Vec<Tok>,
    at: usize,
    vars: &'a dyn Fn(&str) -> Option<usize>,
}

impl T<'_> {
    fn eat(&mut self, op: &str) -> bool {
        if matches!(self.t.get(self.at), Some(Tok::Op(o)) if *o == op) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn bin(&mut self, ops: &[&'static str], next: fn(&mut Self) -> Result<Ex, String>) -> Result<Ex, String> {
        let mut l = next(self)?;
        'outer: loop {
            for &op in ops {
                if self.eat(op) {
                    let r = next(self)?;
                    l = Ex::Bin(op, Box::new(l), Box::new(r));
                    continue 'outer;
                }
            }
            return Ok(l);
        }
    }
    fn or(&mut self) -> Result<Ex, String> {
        self.bin(&["||"], Self::and)
    }
    fn and(&mut self) -> Result<Ex, String> {
        self.bin(&["&&"], Self::cmp)
    }
    fn cmp(&mut self) -> Result<Ex, String> {
        let l = self.sum()?;
        for op in ["==", "!=", "<=", ">=", "<", ">"] {
            if self.eat(op) {
                let r = self.sum()?;
                return Ok(Ex::Bin(op, Box::new(l), Box::new(r)));
            }
        }
        Ok(l)
    }
    fn sum(&mut self) -> Result<Ex, String> {
        self.bin(&["+", "-"], Self::prod)
    }
    fn prod(&mut self) -> Result<Ex, String> {
        self.bin(&["*", "/", "%"], Self::unary)
    }
    fn unary(&mut self) -> Result<Ex, String> {
        if self.eat("!") {
            return Ok(Ex::Not(Box::new(self.unary()?)));
        }
        if self.eat("-") {
            return Ok(Ex::Neg(Box::new(self.unary()?)));
        }
        self.atom()
    }
    fn atom(&mut self) -> Result<Ex, String> {
        let tok = self.t.get(self.at).cloned();
        self.at += 1;
        match tok {
            Some(Tok::Num(n)) => Ok(Ex::Num(n)),
            Some(Tok::Str(s)) => Ok(Ex::Str(s)),
            Some(Tok::Name(n)) => match n.as_str() {
                "true" => Ok(Ex::Bool(true)),
                "false" => Ok(Ex::Bool(false)),
                _ => (self.vars)(&n).map(Ex::Var).ok_or_else(|| format!("no variable called {n}")),
            },
            Some(Tok::Op("(")) => {
                let v = self.or()?;
                if !self.eat(")") {
                    return Err("a ) is missing".into());
                }
                Ok(v)
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("the expression ends too early".into()),
        }
    }
}

// parses `src` into a tree; `vars` gives the index of a variable by name
pub fn parse(src: &str, vars: &dyn Fn(&str) -> Option<usize>) -> Result<Ex, String> {
    let mut p = T { t: tokens(src)?, at: 0, vars };
    let e = p.or()?;
    if p.at < p.t.len() {
        return Err("something is left over after the expression".into());
    }
    Ok(e)
}
