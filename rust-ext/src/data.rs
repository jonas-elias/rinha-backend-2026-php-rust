use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;
use std::sync::OnceLock;

pub struct Dataset {
    pub centroids: *const f32,
    pub offsets: *const u32,
    pub labels: *const u8,
    pub blocks: *const i16,
    pub k: usize,
    pub n: usize,
    pub padded_n: usize,
    _mmap_base: *const u8,
    _mmap_len: usize,
}

unsafe impl Sync for Dataset {}
unsafe impl Send for Dataset {}

static DATASET: OnceLock<Dataset> = OnceLock::new();

pub fn init() -> Result<(), String> {
    let path = std::env::var("INDEX_PATH").unwrap_or_else(|_| "/app/data/index.bin".to_string());
    let ds = Dataset::load_mmap(&path)?;
    DATASET.set(ds).map_err(|_| "dataset already initialized".to_string())
}

pub fn dataset() -> &'static Dataset {
    DATASET.get().expect("dataset not initialized")
}

impl Dataset {
    fn load_mmap(path: &str) -> Result<Self, String> {
        let c_path = CString::new(path).map_err(|e| e.to_string())?;
        unsafe {
            let fd = libc::open(c_path.as_ptr(), libc::O_RDONLY);
            if fd < 0 {
                return Err(format!("open {} failed", path));
            }

            let mut st: libc::stat = std::mem::zeroed();
            if libc::fstat(fd, &mut st) != 0 {
                libc::close(fd);
                return Err("fstat failed".into());
            }
            let len = st.st_size as usize;
            if len < 16 {
                libc::close(fd);
                return Err("file too small".into());
            }

            #[cfg(target_os = "linux")]
            let flags = libc::MAP_SHARED | libc::MAP_POPULATE;
            #[cfg(not(target_os = "linux"))]
            let flags = libc::MAP_SHARED;

            let addr = libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ,
                flags,
                fd,
                0,
            );
            libc::close(fd);
            if addr == libc::MAP_FAILED {
                return Err("mmap failed".into());
            }

            libc::madvise(addr, len, libc::MADV_WILLNEED);

            let base = addr as *const u8;

            let magic = std::slice::from_raw_parts(base, 4);
            if magic != b"IVF1" {
                libc::munmap(addr as *mut c_void, len);
                return Err("bad magic".into());
            }

            let mut p = 4usize;
            let read_u32 = |off: usize| -> u32 {
                let s = std::slice::from_raw_parts(base.add(off), 4);
                u32::from_le_bytes([s[0], s[1], s[2], s[3]])
            };

            let n = read_u32(p) as usize;
            p += 4;
            let k = read_u32(p) as usize;
            p += 4;
            let d = read_u32(p) as usize;
            p += 4;

            if d != 14 {
                libc::munmap(addr as *mut c_void, len);
                return Err(format!("expected d=14 got {}", d));
            }

            let centroids = base.add(p) as *const f32;
            p += d * k * 4;

            let offsets = base.add(p) as *const u32;
            let total_blocks = read_u32(p + k * 4) as usize;
            p += (k + 1) * 4;
            let padded_n = total_blocks * 8;

            let labels = base.add(p) as *const u8;
            p += padded_n;

            let blocks = base.add(p) as *const i16;
            let blocks_bytes = total_blocks * 112 * 2;
            if p + blocks_bytes > len {
                libc::munmap(addr as *mut c_void, len);
                return Err(format!(
                    "blocks overflow: need {} have {}",
                    p + blocks_bytes,
                    len
                ));
            }

            Ok(Dataset {
                centroids,
                offsets,
                labels,
                blocks,
                k,
                n,
                padded_n,
                _mmap_base: base,
                _mmap_len: len,
            })
        }
    }
}

#[cfg(target_os = "macos")]
#[allow(dead_code)]
mod __mac_errno {
    pub unsafe fn errno() -> i32 {
        unsafe { *libc::__error() }
    }
}
