//! Files and folders the operating system hands a running Unluminous, and the macOS half that receives them
//! (`task-2270`).
//!
//! *"files like md should be openable by unluminous, but when I right click and open e.g. an md file I get error
//! that it can't."* The error was macOS's own: *The document "instructions.md" could not be opened. Unluminous
//! cannot open files in the "Markdown Document" format.* `Info.plist` declares the file kinds, so Finder offers
//! Unluminous and sends it the **open documents** Apple event. winit's application delegate answers only
//! `applicationDidFinishLaunching:` and `applicationWillTerminate:`, so AppKit fell back to its own document
//! controller, which has no document class for any type and said the format was not supported. Not one file
//! Finder ever handed Unluminous reached Unluminous's code.
//!
//! So [`listen`] installs a handler for that event. It has to be installed **after** AppKit installs its own
//! handlers, which happens while the application finishes launching, or AppKit's replaces it; Apple's guidance is
//! to install it in `applicationWillFinishLaunching:`. That is winit's delegate method to write, not Unluminous's,
//! so [`listen`] observes the notification of the same name instead, which AppKit posts at the same moment to
//! every observer. The handler reads each file's address out of the event and puts its path on a queue, and the
//! window takes the queue every frame through [`take`].
//!
//! On Windows and Linux a file is handed over on the command line, which `unluminous_app::arguments` already reads,
//! so [`listen`] does nothing there and the queue stays empty.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Set by [`listen`], which only the released binary calls, so a test's window never takes a path another test
/// put on the queue.
static LISTENING: AtomicBool = AtomicBool::new(false);

/// What the operating system has handed over and no window has taken yet.
static WAITING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// The window to wake when something arrives, so it does not wait for the next heartbeat.
static WAKE: OnceLock<egui::Context> = OnceLock::new();

/// Put paths on the queue and wake the window.
pub fn arrived(paths: impl IntoIterator<Item = PathBuf>) {
    if let Ok(mut waiting) = WAITING.lock() {
        waiting.extend(paths);
    }
    if let Some(context) = WAKE.get() {
        context.request_repaint();
    }
}

/// Everything handed over since the last call, oldest first.
pub fn take() -> Vec<PathBuf> {
    WAITING.lock().map(|mut waiting| std::mem::take(&mut *waiting)).unwrap_or_default()
}

/// What the window takes each frame: [`take`], in a process that is listening, and nothing in any other.
pub fn take_for_the_window() -> Vec<PathBuf> {
    match LISTENING.load(Ordering::Relaxed) {
        true => take(),
        false => Vec::new(),
    }
}

/// Which window [`arrived`] wakes. The first call wins, because a process has one window.
pub fn wake_with(context: &egui::Context) {
    let _ = WAKE.set(context.clone());
}

/// Start listening for the files the operating system opens with Unluminous. Called once, before the event loop
/// starts, by the released binary only.
pub fn listen() {
    LISTENING.store(true, Ordering::Relaxed);
    #[cfg(target_os = "macos")]
    mac::listen();
}

#[cfg(target_os = "macos")]
mod mac {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject};
    use objc2::{class, define_class, msg_send, sel, ClassType};
    use objc2_foundation::NSString;

    /// A four character code, which is how an Apple event names its class, its kind and its parameters.
    const fn code(name: &[u8; 4]) -> u32 {
        u32::from_be_bytes(*name)
    }

    /// `kCoreEventClass`, `kAEOpenDocuments` and `keyDirectObject`.
    const CORE_EVENTS: u32 = code(b"aevt");
    const OPEN_DOCUMENTS: u32 = code(b"odoc");
    const DIRECT_OBJECT: u32 = code(b"----");

    define_class!(
        // SAFETY: `NSObject` asks nothing of a subclass, and this one holds no state.
        #[unsafe(super(NSObject))]
        #[name = "UnluminousOpenedBySystem"]
        struct Listener;

        impl Listener {
            /// AppKit has installed its own Apple event handlers, so this one now replaces its open documents
            /// handler rather than being replaced by it.
            #[unsafe(method(applicationWillFinishLaunching:))]
            fn will_finish_launching(&self, _notification: *mut AnyObject) {
                unsafe {
                    let manager: Option<Retained<AnyObject>> =
                        msg_send![class!(NSAppleEventManager), sharedAppleEventManager];
                    if let Some(manager) = manager {
                        let _: () = msg_send![
                            &*manager,
                            setEventHandler: self,
                            andSelector: sel!(handleOpenDocuments:withReplyEvent:),
                            forEventClass: CORE_EVENTS,
                            andEventID: OPEN_DOCUMENTS
                        ];
                    }
                }
            }

            /// Finder, the Dock or `open` asked Unluminous to open these files.
            #[unsafe(method(handleOpenDocuments:withReplyEvent:))]
            fn open_documents(&self, event: *mut AnyObject, _reply: *mut AnyObject) {
                if event.is_null() {
                    return;
                }
                let paths = unsafe { paths_in(&*event) };
                super::arrived(paths);
            }
        }
    );

    /// The paths an open documents event names: a list of file addresses as its direct object.
    unsafe fn paths_in(event: &AnyObject) -> Vec<std::path::PathBuf> {
        let mut paths = Vec::new();
        let list: Option<Retained<AnyObject>> =
            msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT];
        let Some(list) = list else { return paths };
        let count: isize = msg_send![&*list, numberOfItems];
        // A descriptor that is not a list counts no items and is the one file itself.
        let items: Vec<Retained<AnyObject>> = match count {
            0 => vec![list],
            count => (1..=count)
                .filter_map(|index| {
                    let item: Option<Retained<AnyObject>> =
                        msg_send![&*list, descriptorAtIndex: index];
                    item
                })
                .collect(),
        };
        for item in items {
            let url: Option<Retained<AnyObject>> = msg_send![&*item, fileURLValue];
            let Some(url) = url else { continue };
            let path: Option<Retained<NSString>> = msg_send![&*url, path];
            if let Some(path) = path {
                paths.push(std::path::PathBuf::from(path.to_string()));
            }
        }
        paths
    }

    /// Observe `NSApplicationWillFinishLaunchingNotification` with a listener that lives as long as the process.
    pub fn listen() {
        unsafe {
            let listener: Retained<Listener> = msg_send![Listener::class(), new];
            let center: Option<Retained<AnyObject>> =
                msg_send![class!(NSNotificationCenter), defaultCenter];
            let Some(center) = center else { return };
            let name = NSString::from_str("NSApplicationWillFinishLaunchingNotification");
            let _: () = msg_send![
                &*center,
                addObserver: &*listener,
                selector: sel!(applicationWillFinishLaunching:),
                name: &*name,
                object: std::ptr::null::<AnyObject>()
            ];
            // Neither the notification centre nor the Apple event manager keeps the listener alive, and it is
            // wanted for as long as the process runs.
            std::mem::forget(listener);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What arrives is taken once, in the order it arrived.
    #[test]
    fn what_arrives_is_taken_once_in_order() {
        let _ = take();
        arrived([PathBuf::from("/a/one.md"), PathBuf::from("/a/two.md")]);
        arrived([PathBuf::from("/b")]);
        assert_eq!(
            take(),
            vec![PathBuf::from("/a/one.md"), PathBuf::from("/a/two.md"), PathBuf::from("/b")]
        );
        assert!(take().is_empty());
    }
}
