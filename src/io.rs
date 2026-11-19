//! `org.apache.velocity.io.UnicodeInputStream`: a byte stream whose byte order mark tells its encoding, with the
//! mark skipped or kept; and `SimplePool` (`org.apache.velocity.util`).

use std::io::{self, Read};

/// A byte order mark and the encoding it announces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnicodeBom {
    /// The encoding name the mark announces, as Java names it (`UTF-16LE`).
    pub encoding: &'static str,
    /// The mark's bytes.
    pub bytes: &'static [u8],
}

/// `EF BB BF`: UTF-8.
pub const UTF8_BOM: UnicodeBom = UnicodeBom { encoding: "UTF-8", bytes: &[0xef, 0xbb, 0xbf] };
/// `FF FE`: UTF-16, little-endian.
pub const UTF16LE_BOM: UnicodeBom = UnicodeBom { encoding: "UTF-16LE", bytes: &[0xff, 0xfe] };
/// `FE FF`: UTF-16, big-endian.
pub const UTF16BE_BOM: UnicodeBom = UnicodeBom { encoding: "UTF-16BE", bytes: &[0xfe, 0xff] };
/// `FF FE 00 00`: UTF-32, little-endian.
pub const UTF32LE_BOM: UnicodeBom = UnicodeBom { encoding: "UTF-32LE", bytes: &[0xff, 0xfe, 0x00, 0x00] };
/// `00 00 FE FF`: UTF-32, big-endian.
pub const UTF32BE_BOM: UnicodeBom = UnicodeBom { encoding: "UTF-32BE", bytes: &[0x00, 0x00, 0xfe, 0xff] };

const MAX_BOM_SIZE: usize = 4;

/// Reads the first bytes of a stream for a byte order mark, then passes the stream on (without the mark when
/// `skip_bom`).
pub struct UnicodeInputStream<R: Read> {
    pushback: Vec<u8>,
    inner: R,
    encoding: Option<&'static str>,
    skip_bom: bool,
}

impl<R: Read> UnicodeInputStream<R> {
    /// `new UnicodeInputStream(inner, skipBOM)`: reads the byte order mark, if any, right away; `Err` when reading
    /// `inner` fails.
    pub fn new(inner: R, skip_bom: bool) -> io::Result<UnicodeInputStream<R>> {
        let mut s = UnicodeInputStream { pushback: Vec::new(), inner, encoding: None, skip_bom };
        s.encoding = s.read_encoding()?;
        Ok(s)
    }

    /// `isSkipBOM()`: whether the mark is left out of the bytes read.
    pub fn is_skip_bom(&self) -> bool {
        self.skip_bom
    }

    /// `getEncodingFromStream()`: the encoding the mark announces, if there was one.
    pub fn encoding_from_stream(&self) -> Option<&'static str> {
        self.encoding
    }

    fn read_encoding(&mut self) -> io::Result<Option<&'static str>> {
        let mut buf: Vec<u8> = Vec::with_capacity(MAX_BOM_SIZE);
        let mut encoding = None;
        if self.read_byte(&mut buf)? {
            encoding = match buf[0] {
                0x00 => self.matches(&mut buf, UTF32BE_BOM, None)?,
                0xef => self.matches(&mut buf, UTF8_BOM, None)?,
                0xfe => self.matches(&mut buf, UTF16BE_BOM, None)?,
                0xff => match self.matches(&mut buf, UTF16LE_BOM, None)? {
                    Some(e) => self.matches(&mut buf, UTF32LE_BOM, Some(e))?,
                    None => None,
                },
                _ => None,
            };
        }
        // Push the bytes read back, the mark itself aside when skipping it.
        let start = match encoding {
            Some(bom) if self.skip_bom => bom.bytes.len(),
            _ => 0,
        };
        self.pushback = buf[start..].to_vec();
        Ok(encoding.map(|b| b.encoding))
    }

    fn matches(&mut self, buf: &mut Vec<u8>, bom: UnicodeBom, no_match: Option<UnicodeBom>) -> io::Result<Option<UnicodeBom>> {
        for (i, b) in bom.bytes.iter().enumerate() {
            if buf.len() <= i && !self.read_byte(buf)? {
                return Ok(no_match);
            }
            if *b != buf[i] {
                return Ok(no_match);
            }
        }
        Ok(Some(bom))
    }

    fn read_byte(&mut self, buf: &mut Vec<u8>) -> io::Result<bool> {
        let mut b = [0u8; 1];
        if self.inner.read(&mut b)? == 0 {
            return Ok(false);
        }
        if buf.len() >= MAX_BOM_SIZE {
            return Err(io::Error::other("BOM read error"));
        }
        buf.push(b[0]);
        Ok(true)
    }
}

impl<R: Read> Read for UnicodeInputStream<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if !self.pushback.is_empty() {
            let n = self.pushback.len().min(out.len());
            out[..n].copy_from_slice(&self.pushback[..n]);
            self.pushback.drain(..n);
            return Ok(n);
        }
        self.inner.read(out)
    }
}

/// `UnicodeInputStream.sameEncoding`: equal ignoring case, dashes and underscores.
pub fn same_encoding(left: &str, right: &str) -> bool {
    let norm = |s: &str| s.to_uppercase().replace(['-', '_'], "");
    norm(left) == norm(right)
}

/// `org.apache.velocity.util.SimplePool`: a bounded stack of reusable objects.
pub struct SimplePool<T> {
    pool: Vec<Option<T>>,
    current: usize,
}

impl<T> SimplePool<T> {
    /// `new SimplePool(max)`: an empty pool holding at most `max` objects.
    pub fn new(max: usize) -> SimplePool<T> {
        SimplePool { pool: (0..max).map(|_| None).collect(), current: 0 }
    }

    /// `put(o)`: kept while there is room.
    pub fn put(&mut self, o: T) {
        if self.current < self.pool.len() {
            self.pool[self.current] = Some(o);
            self.current += 1;
        }
    }

    /// `get()`: the last one put, if any.
    pub fn get(&mut self) -> Option<T> {
        if self.current == 0 {
            return None;
        }
        self.current -= 1;
        self.pool[self.current].take()
    }

    /// `getPool()`.
    pub fn pool(&self) -> &[Option<T>] {
        &self.pool
    }

    /// `getMax()`.
    pub fn max(&self) -> usize {
        self.pool.len()
    }
}
