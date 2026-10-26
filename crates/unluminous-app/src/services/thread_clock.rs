//! How much processor time the window's thread has had, and which kind of processor it is on.
//!
//! `task-2288` reported that a busy machine makes the window jerky: a drag of the window, a modal opening
//! and a pane resize all stutter while a build holds most of the processors. A frame trace line says how long
//! a frame took on the clock and nothing about why, and on a busy machine there are two very different
//! answers: the frame had more work in it, or the frame had the same work and the thread was waiting for a
//! processor while other programs used them. These two numbers tell the answers apart. The clock time of a
//! frame minus the thread's processor time in it is the time the thread was not running.
//!
//! The processor kind matters on a machine with two kinds of core. The machine this was written on has eight
//! performance cores and sixteen efficient ones, and a frame that is moved onto an efficient core takes
//! longer for the same work.
//!
//! Both are only asked while a frame trace is being written, so the cost is the frame trace's own.

use std::sync::OnceLock;

/// The thread's processor time so far, in milliseconds, or `None` where it cannot be read yet.
///
/// On Windows this is `QueryThreadCycleTime`, which counts at the rate of the processor's time stamp
/// counter, so it is turned into milliseconds with that rate. The rate is measured once on a thread of its
/// own, which takes a tenth of a second, and until then this answers `None`.
pub fn thread_cpu_ms() -> Option<f64> {
    platform::thread_cpu_ms()
}

/// `'P'` while the thread is on one of the machine's fastest processors, `'E'` while it is on a slower one,
/// or `None` where the question cannot be asked. A machine whose processors are all one kind answers `'P'`.
pub fn processor_kind() -> Option<char> {
    platform::processor_kind()
}

/// The time stamp counter's rate in ticks a millisecond, once it has been measured.
static TICKS_PER_MS: OnceLock<f64> = OnceLock::new();

#[cfg(all(windows, target_arch = "x86_64"))]
mod platform {
    use super::TICKS_PER_MS;
    use std::sync::{Once, OnceLock};

    /// Start measuring the counter's rate the first time anything asks.
    static MEASURING: Once = Once::new();

    /// The efficiency class of each logical processor in group 0, and the highest class there is.
    static CLASSES: OnceLock<(Vec<u8>, u8)> = OnceLock::new();

    pub fn thread_cpu_ms() -> Option<f64> {
        MEASURING.call_once(|| {
            std::thread::spawn(|| {
                // The counter runs at a fixed rate whether or not this thread is running, so a sleep
                // measures it as well as a busy loop would, and does not compete with the window.
                let began = std::time::Instant::now();
                let first = unsafe { core::arch::x86_64::_rdtsc() };
                std::thread::sleep(std::time::Duration::from_millis(100));
                let last = unsafe { core::arch::x86_64::_rdtsc() };
                let ms = began.elapsed().as_secs_f64() * 1000.0;
                let _ = TICKS_PER_MS.set((last - first) as f64 / ms);
            });
        });
        let rate = *TICKS_PER_MS.get()?;
        let mut cycles = 0u64;
        let read = unsafe {
            windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime(
                windows_sys::Win32::System::Threading::GetCurrentThread(),
                &mut cycles,
            )
        };
        (read != 0).then(|| cycles as f64 / rate)
    }

    pub fn processor_kind() -> Option<char> {
        let (classes, fastest) = CLASSES.get_or_init(|| {
            let processors = crate::services::scheduling::processors();
            let mut classes = Vec::new();
            for processor in processors {
                let index = usize::from(processor.index);
                if classes.len() <= index {
                    classes.resize(index + 1, 0);
                }
                classes[index] = processor.class;
            }
            let fastest = classes.iter().copied().max().unwrap_or(0);
            (classes, fastest)
        });
        let mut number = windows_sys::Win32::System::Kernel::PROCESSOR_NUMBER::default();
        unsafe { windows_sys::Win32::System::Threading::GetCurrentProcessorNumberEx(&mut number) };
        let class = classes.get(usize::from(number.Number)).copied()?;
        Some(if class == *fastest { 'P' } else { 'E' })
    }
}

#[cfg(unix)]
mod platform {
    pub fn thread_cpu_ms() -> Option<f64> {
        let _ = &super::TICKS_PER_MS;
        let mut time = libc::timespec { tv_sec: 0, tv_nsec: 0 };
        let read = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        (read == 0).then(|| time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1_000_000.0)
    }

    pub fn processor_kind() -> Option<char> {
        None
    }
}

#[cfg(not(any(unix, all(windows, target_arch = "x86_64"))))]
mod platform {
    pub fn thread_cpu_ms() -> Option<f64> {
        let _ = &super::TICKS_PER_MS;
        None
    }

    pub fn processor_kind() -> Option<char> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A thread that spins for a while has used processor time, and the reading never goes backwards.
    #[test]
    fn a_busy_thread_has_used_processor_time() {
        // The rate is measured on a thread of its own, so the first answers can be `None`.
        let began = std::time::Instant::now();
        while thread_cpu_ms().is_none() && cfg!(any(unix, all(windows, target_arch = "x86_64"))) {
            assert!(began.elapsed().as_secs() < 5, "the counter's rate was never measured");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let Some(before) = thread_cpu_ms() else { return };
        // Spin until the counter says 10 ms were used, with five seconds to get there. A fixed 30 ms
        // of the wall clock read as 7 ms on a busy machine, where the thread was not running for most
        // of it (`task-2353`); a counter that never moves still fails.
        let spin = std::time::Instant::now();
        let mut sum = 0u64;
        let mut after = before;
        while after < before + 10.0 && spin.elapsed().as_secs() < 5 {
            for _ in 0..10_000 {
                sum = sum.wrapping_add(std::hint::black_box(1));
            }
            let now = thread_cpu_ms().expect("it answered once");
            assert!(now >= after, "the reading went backwards, from {after} ms to {now} ms");
            after = now;
        }
        assert!(
            after >= before + 10.0,
            "five seconds of spinning read as {} ms ({sum})",
            after - before
        );
    }
}
