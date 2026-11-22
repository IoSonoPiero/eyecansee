//! The stream of a video that is not a file on this computer (phone, server): the `Reader`
//! that reads it a piece at a time, and the callbacks mpv calls to read it ("eyecansee://",
//! `mpv_stream_cb_add_ro`). Split from `mpv.rs` on 2026-11-22.

use std::ffi::{c_char, c_int, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const ERROR_GENERIC: i64 = -20;

/// A video that is not a file on this computer (phone, server), read a piece at a time.
pub trait Reader: Send + Sync {
    fn size(&self) -> u64;
    /// `len` bytes from `offset`, inside the file. Called from mpv's threads: it may block.
    fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String>;
    /// The player closes: a read under way should return at once, and the next ones too.
    fn cancel(&self) {}
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

/// The stream of a player: its `Reader` and what the player needs to know of its reads, shared
/// with mpv's threads (the user data of `mpv_stream_cb_add_ro`, kept by `Mpv`).
pub(crate) struct Stream {
    reader: Arc<dyn Reader>,
    /// The error of the last read of the `Reader`, until one succeeds again: why a video stopped
    /// or did not open (a phone unplugged, a server gone, #209).
    failed: Mutex<Option<String>>,
    /// The player closes: every read from now on fails at once (#276).
    closed: AtomicBool,
}

impl Stream {
    pub(crate) fn new(reader: Arc<dyn Reader>) -> Stream {
        Stream { reader, failed: Mutex::new(None), closed: AtomicBool::new(false) }
    }

    /// The player closes: a read under way is let go (`Reader::cancel`), the next ones fail at
    /// once, so mpv is not kept waiting for a slow phone or server (#276). Called by the player
    /// and by mpv (`cancel_fn`).
    pub(crate) fn cancel(&self) {
        if !self.closed.swap(true, Ordering::Relaxed) {
            self.reader.cancel();
        }
    }

    pub(crate) fn last_error(&self) -> Option<String> {
        self.failed.lock().unwrap().clone()
    }

    fn fetch(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
        if self.closed.load(Ordering::Relaxed) {
            return Err("closed".into());
        }
        let r = self.reader.read(offset, len).and_then(|d| if d.is_empty() { Err("empty read".into()) } else { Ok(d) });
        *self.failed.lock().unwrap() = r.as_ref().err().cloned();
        r
    }
}

/// An open stream: position and the last block read.
struct Cursor {
    stream: Arc<Stream>,
    pos: u64,
    block: (u64, Vec<u8>),
}

impl Cursor {
    fn new(stream: Arc<Stream>) -> Cursor {
        Cursor { stream, pos: 0, block: (0, Vec::new()) }
    }

    fn read(&mut self, out: &mut [u8]) -> Result<usize, String> {
        let size = self.stream.reader.size();
        if self.pos >= size {
            return Ok(0);
        }
        let (start, data) = &self.block;
        if self.pos < *start || self.pos >= start + data.len() as u64 {
            let data = self.stream.fetch(self.pos, BLOCK.min(size - self.pos))?;
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
    // SAFETY: `user` is the pointer of the `Arc<Stream>` kept in `Mpv`, alive as long as the core:
    // one more reference to it is taken here.
    let stream = unsafe {
        Arc::increment_strong_count(user as *const Stream);
        Arc::from_raw(user as *const Stream)
    };
    let cursor = Box::new(Cursor::new(stream));
    // SAFETY: `info` is mpv's, to be filled here.
    unsafe {
        *info = StreamInfo {
            cookie: Box::into_raw(cursor) as *mut c_void,
            read: Some(stream_read),
            seek: Some(stream_seek),
            size: Some(stream_size),
            close: Some(stream_close),
            cancel: Some(stream_cancel),
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
            log!("video stream: ERROR {}", e.replace('\u{1}', ": "));
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
    let cursor = unsafe { &*(cookie as *const Cursor) };
    cursor.stream.reader.size() as i64
}

unsafe extern "C" fn stream_cancel(cookie: *mut c_void) {
    // SAFETY: as `stream_read`; from any thread, only the shared `Stream` is touched.
    let cursor = unsafe { &*(cookie as *const Cursor) };
    cursor.stream.cancel();
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
        let mut c = Cursor::new(Arc::new(Stream::new(Arc::new(Bytes(data.clone())))));
        c.pos = BLOCK - 10;
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

    /// Bytes failing in a range, as a phone that comes back.
    struct Gap(Bytes, std::ops::Range<u64>);

    impl Reader for Gap {
        fn size(&self) -> u64 {
            self.0.size()
        }
        fn read(&self, offset: u64, len: u64) -> Result<Vec<u8>, String> {
            if self.1.contains(&offset) { Err("error-device-gone\u{1}USB".into()) } else { self.0.read(offset, len) }
        }
    }

    /// #209: the error of the last read is kept for the player, until a read succeeds again; the
    /// end of the file reads nothing and changes nothing.
    #[test]
    fn last_read_error_kept() {
        let stream = Arc::new(Stream::new(Arc::new(Gap(Bytes(vec![7; BLOCK as usize * 3]), BLOCK..BLOCK * 2))));
        let mut c = Cursor::new(stream.clone());
        let mut out = vec![0; 10];
        assert_eq!((c.read(&mut out), stream.last_error()), (Ok(10), None));
        c.pos = BLOCK;
        assert!(c.read(&mut out).is_err());
        c.pos = BLOCK * 3;
        assert_eq!(c.read(&mut out), Ok(0));
        assert_eq!(stream.last_error().as_deref(), Some("error-device-gone\u{1}USB"));
        c.pos = BLOCK * 2;
        assert_eq!((c.read(&mut out), stream.last_error()), (Ok(10), None));
    }
}
