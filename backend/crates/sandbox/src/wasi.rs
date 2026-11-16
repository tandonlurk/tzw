//! The guest's whole world: nine hand-written `wasi_snapshot_preview1`
//! functions. stdin/stdout/stderr are in-memory buffers, the environment is
//! empty and the clock is fixed. There is no filesystem, socket, random or
//! real-clock capability to grant; any other import is linked as a trap.

use wasmtime::{Caller, Extern, Linker, Memory, ResourceLimiter, StoreLimits};

const ERRNO_SUCCESS: i32 = 0;
const ERRNO_BADF: i32 = 8;
const ERRNO_FAULT: i32 = 21;
const ERRNO_SPIPE: i32 = 70;
const STDERR_CAP: usize = 4 << 10;

/// `StoreLimits` plus a flag that records whether memory growth was refused,
/// so a trap can be reported as `MemoryLimit`.
pub(crate) struct Limiter {
    pub(crate) inner: StoreLimits,
    pub(crate) memory_hit: bool,
}

impl ResourceLimiter for Limiter {
    fn memory_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        let r = self.inner.memory_growing(current, desired, maximum);
        if !matches!(r, Ok(true)) {
            self.memory_hit = true;
        }
        r
    }
    fn table_growing(
        &mut self,
        current: usize,
        desired: usize,
        maximum: Option<usize>,
    ) -> wasmtime::Result<bool> {
        self.inner.table_growing(current, desired, maximum)
    }
    fn instances(&self) -> usize {
        self.inner.instances()
    }
    fn tables(&self) -> usize {
        self.inner.tables()
    }
    fn memories(&self) -> usize {
        self.inner.memories()
    }
}

/// Per-run state; a fresh one for every `Store`.
pub(crate) struct HostState {
    pub(crate) stdin: Vec<u8>,
    pub(crate) stdin_pos: usize,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stdout_cap: usize,
    pub(crate) stdout_overflow: bool,
    pub(crate) stderr: Vec<u8>,
    pub(crate) now_ns: u64,
    pub(crate) limiter: Limiter,
}

fn memory(caller: &mut Caller<'_, HostState>) -> Option<Memory> {
    match caller.get_export("memory") {
        Some(Extern::Memory(m)) => Some(m),
        _ => None,
    }
}

fn read_u32(mem: &[u8], at: u32) -> Option<u32> {
    let at = at as usize;
    mem.get(at..at.checked_add(4)?)
        .map(|b| u32::from_le_bytes(b.try_into().expect("slice of 4")))
}

fn write_bytes(mem: &mut [u8], at: u32, bytes: &[u8]) -> bool {
    let at = at as usize;
    match mem.get_mut(at..at.saturating_add(bytes.len())) {
        Some(dst) => {
            dst.copy_from_slice(bytes);
            true
        }
        None => false,
    }
}

/// Defines the only host functions the guest can call.
pub(crate) fn link(linker: &mut Linker<HostState>) -> wasmtime::Result<()> {
    const W: &str = "wasi_snapshot_preview1";

    linker.func_wrap(
        W,
        "environ_sizes_get",
        |mut c: Caller<'_, HostState>, n: u32, sz: u32| {
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            let d = m.data_mut(&mut c);
            if write_bytes(d, n, &0u32.to_le_bytes()) && write_bytes(d, sz, &0u32.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;
    linker.func_wrap(
        W,
        "environ_get",
        |_: Caller<'_, HostState>, _: u32, _: u32| ERRNO_SUCCESS,
    )?;

    // A fixed clock: the guest sees only the `now` the host chose.
    linker.func_wrap(
        W,
        "clock_time_get",
        |mut c: Caller<'_, HostState>, _id: u32, _prec: u64, out: u32| {
            let now = c.data().now_ns;
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            if write_bytes(m.data_mut(&mut c), out, &now.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(W, "fd_close", |_: Caller<'_, HostState>, fd: u32| {
        if fd <= 2 { ERRNO_SUCCESS } else { ERRNO_BADF }
    })?;
    linker.func_wrap(
        W,
        "fd_seek",
        |_: Caller<'_, HostState>, _: u32, _: i64, _: u32, _: u32| ERRNO_SPIPE,
    )?;
    linker.func_wrap(
        W,
        "fd_fdstat_get",
        |mut c: Caller<'_, HostState>, fd: u32, out: u32| {
            if fd > 2 {
                return ERRNO_BADF;
            }
            // filetype=CHARACTER_DEVICE(2), flags=0, rights: fd_read(1<<1) | fd_write(1<<6)
            let mut stat = [0u8; 24];
            stat[0] = 2;
            stat[8..16].copy_from_slice(&((1u64 << 1) | (1u64 << 6)).to_le_bytes());
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            if write_bytes(m.data_mut(&mut c), out, &stat) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(
        W,
        "fd_read",
        |mut c: Caller<'_, HostState>, fd: u32, iovs: u32, n_iovs: u32, nread: u32| {
            if fd != 0 {
                return ERRNO_BADF;
            }
            let Some(m) = memory(&mut c) else {
                return ERRNO_FAULT;
            };
            let (mem, st) = m.data_and_store_mut(&mut c);
            let mut total = 0u32;
            for i in 0..n_iovs {
                let base = iovs.wrapping_add(i.wrapping_mul(8));
                let (Some(ptr), Some(len)) =
                    (read_u32(mem, base), read_u32(mem, base.wrapping_add(4)))
                else {
                    return ERRNO_FAULT;
                };
                let rest = &st.stdin[st.stdin_pos..];
                let k = rest.len().min(len as usize);
                if !write_bytes(mem, ptr, &rest[..k]) {
                    return ERRNO_FAULT;
                }
                st.stdin_pos += k;
                total = total.saturating_add(k as u32);
                if k < len as usize {
                    break;
                }
            }
            if write_bytes(mem, nread, &total.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            }
        },
    )?;

    linker.func_wrap(
        W,
        "fd_write",
        |mut c: Caller<'_, HostState>,
         fd: u32,
         iovs: u32,
         n_iovs: u32,
         nwritten: u32|
         -> wasmtime::Result<i32> {
            if fd != 1 && fd != 2 {
                return Ok(ERRNO_BADF);
            }
            let Some(m) = memory(&mut c) else {
                return Ok(ERRNO_FAULT);
            };
            let (mem, st) = m.data_and_store_mut(&mut c);
            let mut total = 0u32;
            for i in 0..n_iovs {
                let base = iovs.wrapping_add(i.wrapping_mul(8));
                let (Some(ptr), Some(len)) =
                    (read_u32(mem, base), read_u32(mem, base.wrapping_add(4)))
                else {
                    return Ok(ERRNO_FAULT);
                };
                let Some(src) = mem.get(ptr as usize..(ptr as usize).saturating_add(len as usize))
                else {
                    return Ok(ERRNO_FAULT);
                };
                if fd == 1 {
                    if st.stdout.len() + src.len() > st.stdout_cap {
                        st.stdout_overflow = true;
                        wasmtime::bail!("guest output exceeds cap");
                    }
                    st.stdout.extend_from_slice(src);
                } else {
                    let room = STDERR_CAP.saturating_sub(st.stderr.len());
                    st.stderr.extend_from_slice(&src[..src.len().min(room)]);
                }
                total = total.saturating_add(len);
            }
            Ok(if write_bytes(mem, nwritten, &total.to_le_bytes()) {
                ERRNO_SUCCESS
            } else {
                ERRNO_FAULT
            })
        },
    )?;

    linker.func_wrap(
        W,
        "proc_exit",
        |_: Caller<'_, HostState>, code: i32| -> wasmtime::Result<()> {
            wasmtime::bail!("guest called proc_exit({code})")
        },
    )?;
    Ok(())
}
