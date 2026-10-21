// A decoder for Kiwi, the binary format of Figma's .fig files (https://github.com/evanw/kiwi): a schema (definitions of enums,
// structs and messages) and a message encoded with it. A .fig file carries its own schema, so the data is decoded without any
// Figma-specific table: values come out as `Val`s keyed by the schema's field names, enums as their names.
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Bytes(Vec<u8>),
    Arr(Vec<Val>),
    Obj(BTreeMap<String, Val>),
    Enum(String),
}

impl Val {
    pub fn get(&self, key: &str) -> Option<&Val> {
        match self {
            Val::Obj(m) => m.get(key),
            _ => None,
        }
    }

    pub fn f(&self, key: &str) -> Option<f64> {
        match self.get(key)? {
            Val::Float(f) => Some(*f),
            Val::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    pub fn i(&self, key: &str) -> Option<i64> {
        match self.get(key)? {
            Val::Int(i) => Some(*i),
            Val::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    pub fn b(&self, key: &str) -> Option<bool> {
        match self.get(key)? {
            Val::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn s(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Val::Str(s) | Val::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn arr(&self, key: &str) -> &[Val] {
        match self.get(key) {
            Some(Val::Arr(a)) => a,
            _ => &[],
        }
    }

    pub fn bytes(&self, key: &str) -> Option<&[u8]> {
        match self.get(key)? {
            Val::Bytes(b) => Some(b),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum DefKind {
    Enum,
    Struct,
    Message,
}

#[derive(Clone, Debug)]
struct Field {
    name: String,
    ty: i32, // negative: a native type (~index into NATIVE), else the index of a definition
    array: bool,
    value: u32, // a message field's id, an enum member's number
}

#[derive(Clone, Debug)]
struct Def {
    name: String,
    kind: DefKind,
    fields: Vec<Field>,
}

const NATIVE: [&str; 8] = ["bool", "byte", "int", "uint", "float", "string", "int64", "uint64"];

pub struct Schema {
    defs: Vec<Def>,
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

type R<T> = Result<T, String>;

impl<'a> Reader<'a> {
    fn byte(&mut self) -> R<u8> {
        let v = *self.b.get(self.at).ok_or("unexpected end of data")?;
        self.at += 1;
        Ok(v)
    }

    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        let s = self.b.get(self.at..self.at + n).ok_or("unexpected end of data")?;
        self.at += n;
        Ok(s)
    }

    // LEB128, up to 64 bits
    fn var_u64(&mut self) -> R<u64> {
        let (mut v, mut shift) = (0u64, 0u32);
        loop {
            let b = self.byte()?;
            if shift < 64 {
                v |= ((b & 0x7f) as u64) << shift;
            }
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 70 {
                return Err("a number is too long".into());
            }
        }
    }

    fn var_u32(&mut self) -> R<u32> {
        Ok(self.var_u64()? as u32)
    }

    // zigzag: 0, -1, 1, -2, ... are 0, 1, 2, 3, ...
    fn var_i32(&mut self) -> R<i32> {
        let v = self.var_u32()?;
        Ok(((v >> 1) as i32) ^ -((v & 1) as i32))
    }

    fn var_i64(&mut self) -> R<i64> {
        let v = self.var_u64()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }

    // Kiwi's float: 0 is one byte, anything else four bytes with the exponent rotated to the front
    fn var_f32(&mut self) -> R<f32> {
        let first = *self.b.get(self.at).ok_or("unexpected end of data")?;
        if first == 0 {
            self.at += 1;
            return Ok(0.0);
        }
        let s = self.take(4)?;
        let bits = u32::from_le_bytes([s[0], s[1], s[2], s[3]]);
        Ok(f32::from_bits((bits << 23) | (bits >> 9)))
    }

    fn string(&mut self) -> R<String> {
        let rest = &self.b[self.at..];
        let n = rest.iter().position(|&c| c == 0).ok_or("a string has no end")?;
        let s = String::from_utf8_lossy(&rest[..n]).into_owned();
        self.at += n + 1;
        Ok(s)
    }
}

impl Schema {
    // the binary schema at the start of a Kiwi file
    pub fn parse(bytes: &[u8]) -> R<Schema> {
        let mut r = Reader { b: bytes, at: 0 };
        let n = r.var_u32()? as usize;
        let mut defs = Vec::with_capacity(n);
        for _ in 0..n {
            let name = r.string()?;
            let kind = match r.byte()? {
                0 => DefKind::Enum,
                1 => DefKind::Struct,
                2 => DefKind::Message,
                k => return Err(format!("unknown definition kind {k}")),
            };
            let count = r.var_u32()? as usize;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                let fname = r.string()?;
                let ty = r.var_i32()?;
                let array = r.byte()? & 1 == 1;
                let value = r.var_u32()?;
                fields.push(Field { name: fname, ty, array, value });
            }
            defs.push(Def { name, kind, fields });
        }
        Ok(Schema { defs })
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.defs.iter().position(|d| d.name == name)
    }

    // decodes `bytes` as the definition called `root` (a .fig file's is "Message")
    pub fn decode(&self, root: &str, bytes: &[u8]) -> R<Val> {
        let mut r = Reader { b: bytes, at: 0 };
        let i = self.index_of(root).ok_or_else(|| format!("the schema has no {root}"))?;
        self.def_value(i, &mut r, 0)
    }

    fn value(&self, ty: i32, r: &mut Reader, depth: usize) -> R<Val> {
        if ty < 0 {
            return Ok(match NATIVE.get((!ty) as usize).copied() {
                Some("bool") => Val::Bool(r.byte()? != 0),
                Some("byte") => Val::Int(r.byte()? as i64),
                Some("int") => Val::Int(r.var_i32()? as i64),
                Some("uint") => Val::Int(r.var_u32()? as i64),
                Some("float") => Val::Float(r.var_f32()? as f64),
                Some("string") => Val::Str(r.string()?),
                Some("int64") => Val::Int(r.var_i64()?),
                Some("uint64") => Val::Int(r.var_u64()? as i64),
                _ => return Err(format!("unknown native type {ty}")),
            });
        }
        self.def_value(ty as usize, r, depth)
    }

    fn def_value(&self, i: usize, r: &mut Reader, depth: usize) -> R<Val> {
        if depth > 64 {
            return Err("the data nests too deeply".into());
        }
        let def = self.defs.get(i).ok_or("a type is out of range")?;
        match def.kind {
            DefKind::Enum => {
                let v = r.var_u32()?;
                Ok(def.fields.iter().find(|f| f.value == v).map_or_else(|| Val::Enum(format!("#{v}")), |f| Val::Enum(f.name.clone())))
            }
            DefKind::Struct => {
                let mut m = BTreeMap::new();
                for f in &def.fields {
                    m.insert(f.name.clone(), self.field(f, r, depth)?);
                }
                Ok(Val::Obj(m))
            }
            DefKind::Message => {
                let mut m = BTreeMap::new();
                loop {
                    let id = r.var_u32()?;
                    if id == 0 {
                        return Ok(Val::Obj(m));
                    }
                    let f = def.fields.iter().find(|f| f.value == id).ok_or_else(|| format!("{} has no field {id}", def.name))?;
                    m.insert(f.name.clone(), self.field(f, r, depth)?);
                }
            }
        }
    }

    fn field(&self, f: &Field, r: &mut Reader, depth: usize) -> R<Val> {
        if !f.array {
            return self.value(f.ty, r, depth + 1);
        }
        let n = r.var_u32()? as usize;
        // a byte array is raw bytes
        if f.ty == !1 {
            return Ok(Val::Bytes(r.take(n)?.to_vec()));
        }
        let mut v = Vec::with_capacity(n.min(1 << 16));
        for _ in 0..n {
            v.push(self.value(f.ty, r, depth + 1)?);
        }
        Ok(Val::Arr(v))
    }
}

#[cfg(test)]
pub mod writer {
    // a small Kiwi encoder for tests: a schema with the given definitions, and values written by hand
    pub fn var_u(mut v: u64, out: &mut Vec<u8>) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return;
            }
            out.push(b | 0x80);
        }
    }
    pub fn var_i(v: i64, out: &mut Vec<u8>) {
        var_u(((v << 1) ^ (v >> 63)) as u64, out);
    }
    pub fn var_f(v: f32, out: &mut Vec<u8>) {
        if v == 0.0 {
            out.push(0);
            return;
        }
        let bits = v.to_bits();
        let rot = (bits >> 23) | (bits << 9);
        out.extend_from_slice(&rot.to_le_bytes());
    }
    pub fn string(s: &str, out: &mut Vec<u8>) {
        out.extend_from_slice(s.as_bytes());
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::writer::*;
    use super::*;

    // enum Color { RED = 1, BLUE = 2 }; struct Point { float x; float y; }; message Thing { string name = 1; Point[] pts = 2; Color color = 3; int n = 4; byte[] raw = 5; bool on = 6; }
    fn schema() -> Vec<u8> {
        let mut b = vec![];
        var_u(3, &mut b);
        // enum
        string("Color", &mut b);
        b.push(0);
        var_u(2, &mut b);
        for (n, v) in [("RED", 1), ("BLUE", 2)] {
            string(n, &mut b);
            var_i(0, &mut b);
            b.push(0);
            var_u(v, &mut b);
        }
        // struct
        string("Point", &mut b);
        b.push(1);
        var_u(2, &mut b);
        for (n, v) in [("x", 1), ("y", 2)] {
            string(n, &mut b);
            var_i(!4, &mut b); // float
            b.push(0);
            var_u(v, &mut b);
        }
        // message
        string("Thing", &mut b);
        b.push(2);
        var_u(6, &mut b);
        for (n, ty, arr, v) in [("name", !5, 0u8, 1), ("pts", 1, 1, 2), ("color", 0, 0, 3), ("n", !2, 0, 4), ("raw", !1, 1, 5), ("on", !0, 0, 6)] {
            string(n, &mut b);
            var_i(ty, &mut b);
            b.push(arr);
            var_u(v, &mut b);
        }
        b
    }

    #[test]
    fn a_message_decodes_with_its_schema() {
        let schema = Schema::parse(&schema()).unwrap();
        let mut m = vec![];
        var_u(1, &mut m);
        string("héllo", &mut m);
        var_u(2, &mut m);
        var_u(2, &mut m);
        for (x, y) in [(1.5f32, -2.25f32), (0.0, 100.0)] {
            var_f(x, &mut m);
            var_f(y, &mut m);
        }
        var_u(3, &mut m);
        var_u(2, &mut m); // BLUE
        var_u(4, &mut m);
        var_i(-300, &mut m);
        var_u(5, &mut m);
        var_u(3, &mut m);
        m.extend_from_slice(&[9, 8, 7]);
        var_u(6, &mut m);
        m.push(1);
        var_u(0, &mut m);
        let v = schema.decode("Thing", &m).unwrap();
        assert_eq!(v.s("name"), Some("héllo"));
        assert_eq!(v.s("color"), Some("BLUE"));
        assert_eq!(v.i("n"), Some(-300));
        assert_eq!(v.bytes("raw"), Some(&[9u8, 8, 7][..]));
        assert_eq!(v.b("on"), Some(true));
        let pts = v.arr("pts");
        assert_eq!((pts[0].f("x"), pts[0].f("y"), pts[1].f("x"), pts[1].f("y")), (Some(1.5), Some(-2.25), Some(0.0), Some(100.0)));
        // truncated data is an error, not a panic
        assert!(schema.decode("Thing", &m[..m.len() - 4]).is_err());
        assert!(schema.decode("Nope", &m).is_err());
    }

    #[test]
    fn varints_and_floats_round_trip() {
        for v in [0i64, 1, -1, 63, -64, 64, 1_000_000, -1_000_000, i32::MAX as i64, i32::MIN as i64] {
            let mut b = vec![];
            var_i(v, &mut b);
            assert_eq!(Reader { b: &b, at: 0 }.var_i64().unwrap(), v);
        }
        for v in [0.0f32, 1.0, -1.0, 0.1, 123456.78, -0.00031, f32::MAX] {
            let mut b = vec![];
            var_f(v, &mut b);
            assert_eq!(Reader { b: &b, at: 0 }.var_f32().unwrap(), v);
        }
    }
}
