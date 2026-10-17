//! Asking the operating system to run the window's thread first when the machine is busy (`task-2288`).
//!
//! *"When my cpu is busy, at 80% ... unluminous becomes jerky when dragging the window around the screen,
//! opening panels, opening modals ... whereas other apps, such as Firefox, do not."* Measured with
//! `tools/perf-smooth/bench.mjs` on the machine that reported it, an eight performance core and sixteen
//! efficient core Core Ultra 9 285, there were two separate faults and neither was the window doing more
//! work:
//!
//! - **With the performance cores busy, the window's thread ran on efficient cores.** The window suite's
//!   processor share takes the fastest nineteen processors, which is every performance core, and Windows
//!   then put 75 to 84% of the window's frames on the five efficient cores left free. Each frame cost about
//!   2.3 times as long there.
//! - **With more busy threads than processors, the window's thread waited its turn.** Every compiler and
//!   test runs at normal priority, so does the window, and a thread at the same priority as twenty-four
//!   others waits for one of them to use up its slice: 12 to 16 ms in the slowest frames, which is two or
//!   three frames of a 164 Hz display.
//!
//! The window's thread is idle nearly all the time and needs a millisecond or two when it is not, so it
//! can be given the processor first at almost no cost to anything else. Chromium does the same for the
//! threads that draw and present (`base/threading/platform_thread_win.cc`, `kPresentation` is
//! `THREAD_PRIORITY_ABOVE_NORMAL` with power throttling off).
//!
//! On macOS the main thread already runs at the user interactive quality of service, which is what the
//! scheduler there uses for the same decision, so nothing is changed.

/// What the window's thread is asked to have. `UNLUMINOUS_SCHEDULING` names one, so a bench can measure
/// each against the others and a machine where one misbehaves has a way out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Favour {
    /// Nothing is changed: normal priority, any processor.
    Off,
    /// A higher priority and no power throttling.
    Priority,
    /// A higher priority, no power throttling, and only the fastest kind of processor on a machine that
    /// has two kinds.
    Cores,
    /// [`Favour::Cores`] at the highest priority a normal process's thread can have, two levels above every
    /// build rather than one.
    Highest,
}

impl Favour {
    /// The choice `UNLUMINOUS_SCHEDULING` names, or the default when it names none.
    ///
    /// @param named - the variable's value, if it is set
    pub fn from_name(named: Option<&str>) -> Self {
        match named.map(str::trim) {
            Some("off") => Favour::Off,
            Some("priority") => Favour::Priority,
            Some("cores") => Favour::Cores,
            Some("highest") => Favour::Highest,
            _ => DEFAULT,
        }
    }
}

/// What a window gets when nothing says otherwise.
///
/// Measured on `task-2288`'s bench against `off`, three interleaved reps under thirty six busy threads on
/// twenty four processors: the slowest 5% of frame gaps while scrolling went from 22.3 to 6.9 ms, while
/// hovering from 22.1 to 7.0 and while typing from 25.5 to 10.9, and Settings opened in 117 ms rather than 221.
/// With the window suite's own load the share of frames on efficient cores went from 0.86 to 0.03 with the
/// priority alone and to 0.00 with the cores as well. At rest nothing moved.
pub const DEFAULT: Favour = Favour::Cores;

/// Ask for the window's thread to be run first. Called once, on the thread that runs the window.
pub fn favour_the_window_thread() {
    let favour = Favour::from_name(std::env::var("UNLUMINOUS_SCHEDULING").ok().as_deref());
    crate::services::frame_trace::mark(match favour {
        Favour::Off => "scheduling-off",
        Favour::Priority => "scheduling-priority",
        Favour::Cores => "scheduling-cores",
        Favour::Highest => "scheduling-highest",
    });
    if favour != Favour::Off {
        platform::favour(favour);
    }
}

/// One logical processor as Windows describes it: the id a CPU set is named by, its number, and how fast
/// a kind of core it is. A higher efficiency class is a faster core.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Processor {
    pub id: u32,
    pub index: u8,
    pub class: u8,
}

/// The CPU set ids of the fastest kind of processor, or nothing on a machine whose processors are all one
/// kind, where asking for a subset would only take processors away.
///
/// @param processors - every logical processor
pub fn fastest(processors: &[Processor]) -> Vec<u32> {
    let Some(top) = processors.iter().map(|processor| processor.class).max() else {
        return Vec::new();
    };
    if processors.iter().all(|processor| processor.class == top) {
        return Vec::new();
    }
    processors
        .iter()
        .filter(|processor| processor.class == top)
        .map(|processor| processor.id)
        .collect()
}

/// Every logical processor in group 0, as Windows describes it. Empty elsewhere.
pub fn processors() -> &'static [Processor] {
    static PROCESSORS: std::sync::OnceLock<Vec<Processor>> = std::sync::OnceLock::new();
    PROCESSORS.get_or_init(platform::processors)
}

#[cfg(windows)]
mod platform {
    use super::{Favour, Processor};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, ProcessPowerThrottling, SetProcessInformation,
        SetThreadInformation, SetThreadPriority, SetThreadSelectedCpuSets, ThreadPowerThrottling,
        PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        PROCESS_POWER_THROTTLING_STATE, THREAD_POWER_THROTTLING_CURRENT_VERSION,
        THREAD_POWER_THROTTLING_EXECUTION_SPEED, THREAD_POWER_THROTTLING_STATE,
        THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_HIGHEST,
    };

    pub fn favour(favour: Favour) {
        unsafe {
            // Base priority 9 against the 8 every build runs at: the window's thread is taken off the queue
            // first whenever it is ready, and it is ready for a millisecond or two a frame.
            let priority = match favour {
                Favour::Highest => THREAD_PRIORITY_HIGHEST,
                _ => THREAD_PRIORITY_ABOVE_NORMAL,
            };
            SetThreadPriority(GetCurrentThread(), priority);
            // Power throttling off for the process and the thread, which is what Windows calls high quality
            // of service: never moved to an efficient core or a low clock to save power.
            let process = PROCESS_POWER_THROTTLING_STATE {
                Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                (&process as *const PROCESS_POWER_THROTTLING_STATE).cast(),
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
            let thread = THREAD_POWER_THROTTLING_STATE {
                Version: THREAD_POWER_THROTTLING_CURRENT_VERSION,
                ControlMask: THREAD_POWER_THROTTLING_EXECUTION_SPEED,
                StateMask: 0,
            };
            SetThreadInformation(
                GetCurrentThread(),
                ThreadPowerThrottling,
                (&thread as *const THREAD_POWER_THROTTLING_STATE).cast(),
                std::mem::size_of::<THREAD_POWER_THROTTLING_STATE>() as u32,
            );
            // The one millisecond timer, which since Windows 10 2004 a process gets only by asking for it
            // itself. egui waits with `request_repaint_after` for the heartbeat, the zoom's glide and the caret,
            // and at the default 15.6 ms an animation frame asked for in six arrives in up to sixteen.
            windows_sys::Win32::Media::timeBeginPeriod(1);
            if matches!(favour, Favour::Cores | Favour::Highest) {
                let ids = super::fastest(super::processors());
                if !ids.is_empty() {
                    SetThreadSelectedCpuSets(GetCurrentThread(), ids.as_ptr(), ids.len() as u32);
                }
            }
        }
    }

    pub fn processors() -> Vec<Processor> {
        use windows_sys::Win32::System::SystemInformation::{
            GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
        };
        let mut length = 0u32;
        unsafe {
            GetSystemCpuSetInformation(
                std::ptr::null_mut(),
                0,
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        let mut buffer = vec![0u8; length as usize];
        let read = unsafe {
            GetSystemCpuSetInformation(
                buffer.as_mut_ptr().cast(),
                length,
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        let mut found = Vec::new();
        if read == 0 {
            return found;
        }
        let mut at = 0usize;
        while at + std::mem::size_of::<SYSTEM_CPU_SET_INFORMATION>() <= length as usize {
            // The entries are packed at their own `Size`, which can be larger than the struct, so each is
            // read where it starts rather than as an element of an array.
            let entry: SYSTEM_CPU_SET_INFORMATION =
                unsafe { std::ptr::read_unaligned(buffer.as_ptr().add(at).cast()) };
            if entry.Size == 0 {
                break;
            }
            let set = unsafe { entry.Anonymous.CpuSet };
            if set.Group == 0 {
                found.push(Processor {
                    id: set.Id,
                    index: set.LogicalProcessorIndex,
                    class: set.EfficiencyClass,
                });
            }
            at += entry.Size as usize;
        }
        found
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{Favour, Processor};

    pub fn favour(_favour: Favour) {}

    pub fn processors() -> Vec<Processor> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn processor(id: u32, class: u8) -> Processor {
        Processor { id, index: id as u8, class }
    }

    /// On a machine with two kinds of core the fastest kind is chosen, and on a machine with one kind
    /// nothing is, because a subset of identical processors only takes processors away.
    #[test]
    fn the_fastest_kind_is_chosen_only_where_there_are_two_kinds() {
        let hybrid = [processor(0, 1), processor(1, 1), processor(2, 0), processor(3, 0)];
        assert_eq!(fastest(&hybrid), vec![0, 1]);
        let uniform = [processor(0, 0), processor(1, 0)];
        assert!(fastest(&uniform).is_empty());
        assert!(fastest(&[]).is_empty());
    }

    /// The variable names each choice, and anything else is the default.
    #[test]
    fn the_variable_names_a_choice_and_anything_else_is_the_default() {
        assert_eq!(Favour::from_name(Some("off")), Favour::Off);
        assert_eq!(Favour::from_name(Some("priority")), Favour::Priority);
        assert_eq!(Favour::from_name(Some(" cores ")), Favour::Cores);
        assert_eq!(Favour::from_name(Some("highest")), Favour::Highest);
        assert_eq!(Favour::from_name(Some("fast")), DEFAULT);
        assert_eq!(Favour::from_name(None), DEFAULT);
    }
}
