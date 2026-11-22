//! The stream of a video that is not a file on this computer (phone, server): the `Reader`
//! that reads it a piece at a time, and the callbacks mpv calls to read it ("eyecansee://",
//! `mpv_stream_cb_add_ro`). Split from `mpv.rs` on 2026-11-22.

use std::ffi::{c_char, c_int, c_void};
use std::sync::Arc;

const ERROR_GENERIC: i64 = -20;

/// A video that is not a file on this computer (phone, server), read a piece at a time.
pub trait Reader: Send + Sync {
    fn size(&self) -> u64;
    /// `len` bytes from `offset`, inside the file. Called from mpv's threads: it may block.
    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String>;
}

/// mpv_stream_cb_info (stream_cb.h).
#[repr(C)]
pub(crate) struct StreamInfo {
    cookie: *mut c_void,
    read: Option<unsafe extern "C" fn(*mut c_void, *mut c_char, u64) -> i64>,
    seek: Option<unsafe extern "C" fn(*mut c_void, i64) -> i64>,
    size: Option<unsafe extern "C" fn(*mut c_void) -> i64>,
    close: Option<unsafe extern "C" fn(*mut c_void)>,
    cancel: Option<unsafe extern "C" fn(*mut c_void)>,
}

pub(crate) type OpenFn = unsafe extern "C" fn(*mut c_void, *mut c_char, *mut StreamInfo) -> c_int;

/// Bytes asked of the `Reader` at a time: mpv reads in small pieces, a phone answers each request slowly.
// ponytail: one block kept, no read-ahead; mpv's own cache covers playback
const BLOCK: u64 = 1 << 20;

/// An open stream: position and the last block read.
struct Cursor {
    reader: Arc<dyn Reader>,
    pos: u64,
    block: (u64, Vec<u8>),
}

impl Cursor {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, String> {
        let size = self.reader.size();
        if self.pos >= size {
            return Ok(0);
        }
        let (start, data) = &self.block;
        if self.pos < *start || self.pos >= start + data.len() as u64 {
            let data = self.reader.read(self.pos, BLOCK.min(size - self.pos))?;
            if data.is_empty() {
                return Err("empty read".into());
            }
            self.block = (self.pos, data);
        }
        let (start, data) = &self.block;
        let from = &data[(self.pos - start) as usize..];
        let n = from.len().min(out.len());
        out[..n].copy_from_slice(&from[..n]);
        self.pos += n as u64;
        Ok(n)
    }
}

pub(crate) unsafe extern "C" fn stream_open(user: *mut c_void, _uri: *mut c_char, info: *mut StreamInfo) -> c_int {
    // SAFETY: `user` is the `Arc<dyn Reader>` boxed in `Mpv`, alive as long as the core.
    let reader = unsafe { (*(user as *const Arc<dyn Reader>)).clone() };
    let cursor = Box::new(Cursor { reader, pos: 0, block: (0, Vec::new()) });
    // SAFETY: `info` is mpv's, to be filled here.
    unsafe {
        *info = StreamInfo {
            cookie: Box::into_raw(cursor) as *mut c_void,
            read: Some(stream_read),
            seek: Some(stream_seek),
            size: Some(stream_size),
            close: Some(stream_close),
            cancel: None,
        };
    }
    0
}

unsafe extern "C" fn stream_read(cookie: *mut c_void, buf: *mut c_char, n: u64) -> i64 {
    // SAFETY: the cookie is the `Cursor` of `stream_open`, used by one mpv thread at a time;
    // `buf` holds `n` bytes.
    let (cursor, out) = unsafe { (&mut *(cookie as *mut Cursor), std::slice::from_raw_parts_mut(buf as *mut u8, n as usize)) };
    match cursor.read(out) {
        Ok(n) => n as i64,
        Err(e) => {
            log!("video stream: ERROR {e}");
            -1
        }
    }
}

unsafe extern "C" fn stream_seek(cookie: *mut c_void, offset: i64) -> i64 {
    // SAFETY: as `stream_read`.
    let cursor = unsafe { &mut *(cookie as *mut Cursor) };
    if offset < 0 {
        return ERROR_GENERIC;
    }
    cursor.pos = offset as u64;
    offset
}

unsafe extern "C" fn stream_size(cookie: *mut c_void) -> i64 {
    // SAFETY: as `stream_read`.
    unsafe { (*(cookie as *const Cursor)).reader.size() as i64 }
}

unsafe extern "C" fn stream_close(cookie: *mut c_void) {
    // SAFETY: the box of `stream_open`, closed once.
    drop(unsafe { Box::from_raw(cookie as *mut Cursor) });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file read through `Reader`, as a phone would be.
    struct Bytes(Vec<u8>);

    impl Reader for Bytes {
        fn size(&self) -> u64 {
            self.0.len() as u64
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            Ok(self.0[offset as usize..(offset + len) as usize].to_vec())
        }
    }

    #[test]
    fn cursor_reads_across_blocks() {
        let data: Vec<u8> = (0..BLOCK as usize * 2 + 100).map(|i| (i % 251) as u8).collect();
        let mut c = Cursor { reader: Arc::new(Bytes(data.clone())), pos: BLOCK - 10, block: (0, Vec::new()) };
        let mut out = vec![0; 30];
        assert_eq!(c.read(&mut out), Ok(30)); // block read from BLOCK - 10
        assert_eq!(out, data[BLOCK as usize - 10..BLOCK as usize + 20]);
        c.pos = BLOCK * 2 - 15;
        assert_eq!(c.read(&mut out), Ok(5)); // to the end of that block
        assert_eq!(c.read(&mut out), Ok(30)); // the next one
        assert_eq!(out, data[BLOCK as usize * 2 - 10..BLOCK as usize * 2 + 20]);
        c.pos = BLOCK * 2 + 90;
        assert_eq!(c.read(&mut out), Ok(10));
        assert_eq!(out[..10], data[data.len() - 10..]);
        assert_eq!(c.read(&mut out), Ok(0)); // end of file
    }
}
