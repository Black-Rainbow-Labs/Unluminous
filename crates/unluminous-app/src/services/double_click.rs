//! How long the window waits for a second click before it stops being a double click.
//!
//! `task-2280`: *"Double clicking for text/word selection still feels off."* egui counts two presses as a
//! double click when the second comes within 0.3 seconds of the first. Windows lets a person choose the
//! interval in the mouse settings and starts it at 0.5 seconds, so a double click at the pace every other
//! program on the machine accepts arrived as two single clicks: the caret moved twice and no word was
//! selected. The window now asks the system for the interval and gives egui that.

/// The interval in seconds, as the operating system has it set.
///
/// Windows answers with `GetDoubleClickTime`. Elsewhere the half second macOS starts at, which is also
/// what Windows starts at.
pub fn system_interval() -> f64 {
    #[cfg(windows)]
    {
        // SAFETY: no arguments, no pointers, and it cannot fail.
        let milliseconds = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() };
        if milliseconds > 0 {
            return f64::from(milliseconds) / 1000.0;
        }
    }
    0.5
}

/// Give egui the system's interval, kept between a fifth of a second and a second and a half so a setting
/// out of that range cannot make every click a double click or none of them one.
pub fn follow_the_system(ctx: &egui::Context) {
    let interval = system_interval().clamp(0.2, 1.5);
    ctx.options_mut(|options| options.input_options.max_double_click_delay = interval);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interval_is_a_sensible_number_of_seconds() {
        let interval = system_interval();
        assert!((0.05..=5.0).contains(&interval), "{interval}");
    }
}
