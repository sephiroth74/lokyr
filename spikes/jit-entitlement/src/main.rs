//! What the hardened runtime's `allow-jit` changes in a process that maps no
//! code of its own, as Lokyr's latency's tail found: the same measures in
//! the same binary, signed of each entitlement, the binary's name telling
//! which. Each measure's distribution in nanoseconds, a line of JSON.
//!
//! - `pingpong`: the round trip of a wake between two threads, as a key's
//!   event goes from the main thread to a plugin's or the PTY's and back.
//! - `wake`: how late a sleep of 1 ms wakes, at rest and beside a busy
//!   thread on each core, as a frame's timer under load.
//! - `faults`: a page's first touch, of anonymous memory.
//! - `syscall`: `getppid`.
//! - `map`: whether executable memory maps, of `MAP_JIT` and without it.
//! - With `--jit-region`, a region of `MAP_JIT` mapped before the measures,
//!   as Luau's code generator maps one, where the signature allows it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

fn stats(name: &str, mut samples: Vec<u64>) {
    samples.sort_unstable();
    let at = |share: f64| samples[((samples.len() - 1) as f64 * share).round() as usize];
    println!(
        "{{\"measure\":\"{name}\",\"n\":{},\"p50\":{},\"p90\":{},\"p99\":{},\"p999\":{},\"max\":{}}}",
        samples.len(),
        at(0.5),
        at(0.9),
        at(0.99),
        at(0.999),
        samples[samples.len() - 1]
    );
}

fn pingpong(rounds: usize) -> Vec<u64> {
    let pair = Arc::new((Mutex::new(0u64), Condvar::new()));
    let other = Arc::clone(&pair);
    let echo = std::thread::spawn(move || {
        let (lock, wake) = &*other;
        let mut seen = 0;
        loop {
            let mut turn = lock.lock().unwrap();
            while *turn == seen || *turn % 2 == 0 {
                turn = wake.wait(turn).unwrap();
            }
            if *turn == u64::MAX {
                return;
            }
            *turn += 1;
            seen = *turn;
            wake.notify_one();
        }
    });
    let (lock, wake) = &*pair;
    let mut samples = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let start = Instant::now();
        let mut turn = lock.lock().unwrap();
        *turn += 1;
        let sent = *turn;
        wake.notify_one();
        while *turn == sent {
            turn = wake.wait(turn).unwrap();
        }
        drop(turn);
        samples.push(start.elapsed().as_nanos() as u64);
    }
    *lock.lock().unwrap() = u64::MAX;
    wake.notify_one();
    echo.join().unwrap();
    samples
}

fn wake(rounds: usize, busy: usize) -> Vec<u64> {
    let stop = Arc::new(AtomicBool::new(false));
    let spinners: Vec<_> = (0..busy)
        .map(|_| {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut x = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                    std::hint::black_box(x);
                }
            })
        })
        .collect();
    let mut samples = Vec::with_capacity(rounds);
    for _ in 0..rounds {
        let start = Instant::now();
        std::thread::sleep(Duration::from_millis(1));
        samples.push((start.elapsed().as_nanos() as u64).saturating_sub(1_000_000));
    }
    stop.store(true, Ordering::Relaxed);
    for spinner in spinners {
        spinner.join().unwrap();
    }
    samples
}

fn faults(pages: usize) -> Vec<u64> {
    let page = 16 * 1024;
    let size = pages * page;
    // SAFETY: an anonymous private mapping of `size` bytes, unmapped below.
    let memory = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    assert_ne!(memory, libc::MAP_FAILED);
    let base = memory.cast::<u8>();
    let mut samples = Vec::with_capacity(pages);
    for index in 0..pages {
        let start = Instant::now();
        // SAFETY: within the mapping.
        unsafe { base.add(index * page).write_volatile(1) };
        samples.push(start.elapsed().as_nanos() as u64);
    }
    // SAFETY: the mapping made above.
    unsafe { libc::munmap(memory, size) };
    samples
}

fn syscalls(rounds: usize) -> Vec<u64> {
    let mut samples = Vec::with_capacity(rounds / 100);
    for _ in 0..rounds / 100 {
        let start = Instant::now();
        for _ in 0..100 {
            // SAFETY: no arguments, no effect.
            std::hint::black_box(unsafe { libc::getppid() });
        }
        samples.push(start.elapsed().as_nanos() as u64 / 100);
    }
    samples
}

/// Whether a page maps of `flags` and turns executable.
fn maps(flags: libc::c_int) -> String {
    let size = 16 * 1024;
    // SAFETY: an anonymous private mapping, unmapped below.
    let memory = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON | flags,
            -1,
            0,
        )
    };
    if memory == libc::MAP_FAILED {
        return format!("mmap: {}", std::io::Error::last_os_error());
    }
    // SAFETY: the mapping made above.
    let executable = unsafe { libc::mprotect(memory, size, libc::PROT_READ | libc::PROT_EXEC) };
    let told = if executable == 0 {
        "executable".to_owned()
    } else {
        format!("mprotect: {}", std::io::Error::last_os_error())
    };
    // SAFETY: the mapping made above.
    unsafe { libc::munmap(memory, size) };
    told
}

/// Maps a page of `flags`, writes an instruction that returns into it,
/// turns it executable and calls it: whether the process lives through it
/// tells whether the signature lets code of its own run.
fn run_code(flags: libc::c_int) {
    let size = 16 * 1024;
    // SAFETY: an anonymous private mapping, kept until the process ends.
    let memory = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON | flags,
            -1,
            0,
        )
    };
    assert_ne!(memory, libc::MAP_FAILED, "{}", std::io::Error::last_os_error());
    // `ret`, of arm64.
    // SAFETY: within the mapping, writable.
    unsafe { memory.cast::<u32>().write(0xd65f_03c0) };
    // SAFETY: the mapping made above.
    let turned = unsafe { libc::mprotect(memory, size, libc::PROT_READ | libc::PROT_EXEC) };
    assert_eq!(turned, 0, "{}", std::io::Error::last_os_error());
    unsafe extern "C" {
        fn sys_icache_invalidate(start: *mut libc::c_void, len: usize);
    }
    // SAFETY: the page holds a whole function, `ret`, executable.
    unsafe {
        sys_icache_invalidate(memory, size);
        let function: extern "C" fn() = std::mem::transmute(memory);
        function();
    }
}

/// Whether code of its own runs in a process of this binary's signature, of
/// `flags`: the child's end.
fn runs_code(mode: &str) -> String {
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--run-code", mode])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    if status.success() {
        "runs".to_owned()
    } else {
        format!("fails: {status}")
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let [_, flag, mode] = &args[..]
        && flag == "--run-code"
    {
        run_code(if mode == "jit" { libc::MAP_JIT } else { 0 });
        return;
    }
    let name = std::path::Path::new(&args[0])
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let region = args.iter().any(|arg| arg == "--jit-region");
    let cores = std::thread::available_parallelism().map_or(8, usize::from);
    println!(
        "{{\"binary\":\"{name}\",\"map_jit\":\"{}\",\"map_plain\":\"{}\",\"code_jit\":\"{}\",\"code_plain\":\"{}\"}}",
        maps(libc::MAP_JIT),
        maps(0),
        runs_code("jit"),
        runs_code("plain")
    );
    let _kept = region.then(|| {
        // SAFETY: an anonymous private mapping of `MAP_JIT`, kept until the
        // process ends.
        let memory = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                1 << 20,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            )
        };
        println!("{{\"jit_region\":{}}}", memory != libc::MAP_FAILED);
        memory as usize
    });
    stats("pingpong", pingpong(20_000));
    stats("wake-rest", wake(1000, 0));
    stats("wake-busy", wake(1000, cores));
    stats("faults", faults(16_384));
    stats("syscall", syscalls(1_000_000));
}
