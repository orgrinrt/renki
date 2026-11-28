//! The reader: one pass over the text, recursive descent for values.

use crate::index::{Index, Kind, NONE};
use crate::key::{self, KeyPath};
use crate::string::{self, Str};
use crate::{Error, ErrorKind, Node, Options, Scalar, Version, Visitor, number};

pub(crate) struct Parser<'s, 'n, 'v, V> {
    src:     &'s str,
    b:       &'s [u8],
    pos:     usize,
    options: Options,
    index:   Index<'s, 'n>,
    visitor: &'v mut V,
    /// The table pairs go into: the root (`NONE`) until the first header.
    table:   u32,
}

impl<'s, 'n, 'v, V: Visitor<'s>> Parser<'s, 'n, 'v, V> {
    pub(crate) fn new(
        src: &'s str,
        options: Options,
        nodes: &'n mut [Node],
        visitor: &'v mut V,
    ) -> Self {
        let pos = if src.starts_with('\u{feff}') { 3 } else { 0 };
        Self {
            src,
            b: src.as_bytes(),
            pos,
            options,
            index: Index::new(src, nodes),
            visitor,
            table: NONE,
        }
    }

    const fn err(&self, kind: ErrorKind) -> Error {
        Error {
            kind,
            at: self.pos,
        }
    }

    fn lift<T>(&mut self, r: Result<(T, usize), (ErrorKind, usize)>) -> Result<T, Error> {
        match r {
            Ok((t, end)) => {
                self.pos = end;
                Ok(t)
            },
            Err((kind, at)) => {
                Err(Error {
                    kind,
                    at,
                })
            },
        }
    }

    fn peek(&self) -> Option<u8> {
        self.b.get(self.pos).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        let hit = self.peek() == Some(c);
        self.pos += usize::from(hit);
        hit
    }

    fn expect(&mut self, c: u8, what: &'static str) -> Result<(), Error> {
        if self.eat(c) { Ok(()) } else { Err(self.err(ErrorKind::Expected(what))) }
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }

    /// A `#` comment up to, not including, its newline.
    fn comment(&mut self) -> Result<(), Error> {
        self.pos += 1;
        loop {
            match self.peek() {
                None | Some(b'\n') => return Ok(()),
                Some(b'\r') if self.b.get(self.pos + 1) == Some(&b'\n') => return Ok(()),
                Some(c) if (c < 0x20 && c != b'\t') || c == 0x7F => {
                    return Err(self.err(ErrorKind::ControlCharacter));
                },
                Some(_) => self.pos += 1,
            }
        }
    }

    /// A newline, or the end of the text.
    fn newline(&mut self) -> Result<bool, Error> {
        match self.peek() {
            None => Ok(true),
            Some(b'\n') => {
                self.pos += 1;
                Ok(true)
            },
            Some(b'\r') if self.b.get(self.pos + 1) == Some(&b'\n') => {
                self.pos += 2;
                Ok(true)
            },
            Some(b'\r') => Err(self.err(ErrorKind::BareCarriageReturn)),
            Some(_) => Ok(false),
        }
    }

    /// Spaces, newlines and comments, where an array allows them, and an
    /// inline table under TOML 1.1.
    fn blank(&mut self) -> Result<(), Error> {
        loop {
            self.ws();
            if self.peek() == Some(b'#') {
                self.comment()?;
            }
            if self.peek().is_none() || !self.newline()? {
                return Ok(());
            }
        }
    }

    pub(crate) fn document(mut self) -> Result<usize, Error> {
        loop {
            self.ws();
            match self.peek() {
                None => return Ok(self.index.peak() as usize),
                Some(b'#') => self.comment()?,
                Some(b'\n' | b'\r') => {},
                Some(b'[') => self.header()?,
                Some(_) => {
                    let table = self.table;
                    self.pair(table, 0)?;
                },
            }
            self.ws();
            if self.peek() == Some(b'#') {
                self.comment()?;
            }
            if !self.newline()? {
                return Err(self.err(ErrorKind::Expected("newline")));
            }
        }
    }

    fn header(&mut self) -> Result<(), Error> {
        self.pos += 1;
        let array = self.eat(b'[');
        self.ws();
        let at = self.pos;
        let path = key::path(self.src, at, self.options.version);
        let path = self.lift(path)?;
        self.ws();
        self.expect(b']', "]")?;
        if array {
            self.expect(b']', "]]")?;
        }
        self.define_header(path, array)?;
        if array {
            self.visitor.array_of_tables_header(path);
        } else {
            self.visitor.table_header(path);
        }
        Ok(())
    }

    fn define_header(&mut self, path: KeyPath<'s>, array: bool) -> Result<(), Error> {
        let mut parent = NONE;
        let mut keys = path.keys().peekable();
        while let Some(k) = keys.next() {
            let last = keys.peek().is_none();
            let hash = k.hash();
            let at = k.raw().as_ptr() as usize - self.src.as_ptr() as usize;
            let refuse = |kind| {
                Err(Error {
                    kind,
                    at,
                })
            };
            let Some(n) = self.index.find(parent, &k, hash) else {
                parent = match (last, array) {
                    (false, _) => self.index.child(parent, &k, hash, Kind::Implicit)?,
                    (true, false) => self.index.child(parent, &k, hash, Kind::Header)?,
                    (true, true) => {
                        let a = self.index.child(parent, &k, hash, Kind::ArrayOfTables)?;
                        self.index.element(Some(a), at)?
                    },
                };
                continue;
            };
            parent = match (self.index.kind(n), last, array) {
                (Kind::Implicit | Kind::Header | Kind::Dotted, false, _) => n,
                (Kind::ArrayOfTables, false, _) => self.index.last_element(n),
                (Kind::Implicit, true, false) => {
                    self.index.set_kind(n, Kind::Header);
                    n
                },
                (Kind::ArrayOfTables, true, true) => self.index.element(Some(n), at)?,
                (Kind::Header, true, false) => return refuse(ErrorKind::DuplicateTable),
                _ => return refuse(ErrorKind::CannotExtend),
            };
        }
        self.table = parent;
        Ok(())
    }

    /// `key = value`, into `table`.
    fn pair(&mut self, table: u32, depth: u32) -> Result<(), Error> {
        let at = self.pos;
        let path = key::path(self.src, at, self.options.version);
        let path = self.lift(path)?;
        self.ws();
        self.expect(b'=', "=")?;
        self.ws();
        let mut parent = table;
        let mut keys = path.keys().peekable();
        while let Some(k) = keys.next() {
            let last = keys.peek().is_none();
            let hash = k.hash();
            let at = k.raw().as_ptr() as usize - self.src.as_ptr() as usize;
            parent = match (self.index.find(parent, &k, hash), last) {
                (None, false) => self.index.child(parent, &k, hash, Kind::Dotted)?,
                (None, true) => self.index.child(parent, &k, hash, Kind::Value)?,
                (Some(n), false) if self.index.kind(n) == Kind::Dotted => n,
                (Some(_), false) => {
                    return Err(Error {
                        kind: ErrorKind::CannotExtend,
                        at,
                    });
                },
                (Some(_), true) => {
                    return Err(Error {
                        kind: ErrorKind::DuplicateKey,
                        at,
                    });
                },
            };
        }
        self.visitor.key(path);
        let mark = self.index.len();
        self.value(Some(parent), depth)?;
        self.index.forget(mark, Some(parent));
        Ok(())
    }

    /// A value; an inline table among them fills `owner`, or a new element.
    fn value(&mut self, owner: Option<u32>, depth: u32) -> Result<(), Error> {
        let v = self.options.version;
        let at = self.pos;
        let s: Result<(Str<'s>, usize), _> = match self.peek() {
            Some(b'[') => return self.array(depth + 1),
            Some(b'{') => return self.inline_table(owner, depth + 1),
            Some(q @ (b'"' | b'\'')) if self.b[at ..].starts_with(&[q, q, q]) => {
                string::multiline(self.src, at, q, v)
            },
            Some(b'"') => string::basic(self.src, at, v),
            Some(b'\'') => string::literal(self.src, at),
            _ => {
                let end = number::token_end(self.b, at);
                if end == at {
                    return Err(self.err(ErrorKind::Expected("value")));
                }
                let scalar = number::scalar(&self.b[at .. end], v).map_err(|kind| {
                    Error {
                        kind,
                        at,
                    }
                })?;
                self.pos = end;
                self.visitor.scalar(scalar);
                return Ok(());
            },
        };
        let s = self.lift(s)?;
        self.visitor.scalar(Scalar::String(s));
        Ok(())
    }

    fn deeper(&self, depth: u32) -> Result<(), Error> {
        if depth > self.options.max_depth {
            Err(self.err(ErrorKind::TooDeep))
        } else {
            Ok(())
        }
    }

    fn array(&mut self, depth: u32) -> Result<(), Error> {
        self.deeper(depth)?;
        self.pos += 1;
        self.visitor.begin_array();
        let mark = self.index.len();
        loop {
            self.blank()?;
            if self.eat(b']') {
                break;
            }
            self.value(None, depth)?;
            self.index.forget(mark, None);
            self.blank()?;
            if self.eat(b']') {
                break;
            }
            self.expect(b',', ", or ]")?;
        }
        self.visitor.end_array();
        Ok(())
    }

    fn inline_table(&mut self, owner: Option<u32>, depth: u32) -> Result<(), Error> {
        self.deeper(depth)?;
        let table = match owner {
            Some(o) => o,
            None => self.index.element(None, self.pos)?,
        };
        self.pos += 1;
        self.visitor.begin_table();
        let v11 = self.options.version == Version::V1_1_0;
        let gap = |p: &mut Self| {
            if v11 {
                return p.blank();
            }
            p.ws();
            Ok(())
        };
        gap(self)?;
        if !self.eat(b'}') {
            loop {
                self.pair(table, depth)?;
                gap(self)?;
                if self.eat(b'}') {
                    break;
                }
                self.expect(b',', ", or }")?;
                gap(self)?;
                if v11 && self.eat(b'}') {
                    break;
                }
            }
        }
        self.visitor.end_table();
        Ok(())
    }
}
