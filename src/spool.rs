//! The temporary copy --flexible=widest makes of an input it cannot read twice.

use std::error::Error;

/// Holds a temporary file for as long as it is needed and removes it again on
/// drop, including when an error unwinds past it.
pub(crate) struct TempSpool {
    /// Where the copy is.
    pub(crate) path: std::path::PathBuf,
}

impl Drop for TempSpool {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        // Only after the file is gone, so a signal landing in between finds
        // the name still armed and removes it itself. The other order leaves
        // a window where neither does. Nothing else can be at the name to
        // remove by mistake: it carries this process's id and a part nobody
        // could predict, and this process is still alive to hold it.
        #[cfg(unix)]
        disarm_spool_signals();
    }
}

/// Removing the spool when a signal ends the run.
///
/// Drop covers every ending Rust knows about (a clean finish, an error on its
/// way out, a panic unwinding), but a signal is not one of them: the process
/// stops where it stands and no destructor runs. Without this, Ctrl-C during
/// the copy, or the SIGTERM a job scheduler sends when a run overruns its
/// slot, would leave the spool in the temporary directory holding the whole of
/// the input, which is usually a log. The help promises the file is removed
/// when the run ends, including if it fails, and a long run is what this mode
/// is for.
///
/// The handler may only do async-signal-safe things, which rules out the
/// allocator and so the PathBuf itself. The name is therefore laid down as a
/// NUL-terminated string before the handler can ever run, and reached through
/// an atomic pointer; unlink(2) is on the safe list.
#[cfg(unix)]
pub(crate) static SPOOL_NAME: std::sync::atomic::AtomicPtr<libc::c_char> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());

/// The signals that end a run in the ordinary course of things: Ctrl-C, a
/// scheduler's polite stop, and a lost terminal. SIGQUIT is left alone because
/// it is asked for when someone wants the core dump as it fell, and SIGKILL
/// cannot be caught by anyone; see the note in --flexible's help.
#[cfg(unix)]
pub(crate) const SPOOL_SIGNALS: [libc::c_int; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

#[cfg(unix)]
/// The handler for SPOOL_SIGNALS: removes the spool, then lets the signal
/// end the run as it would have.
pub(crate) extern "C" fn remove_spool_on_signal(signal: libc::c_int) {
    use std::sync::atomic::Ordering;
    let name = SPOOL_NAME.swap(std::ptr::null_mut(), Ordering::SeqCst);
    if !name.is_null() {
        // SAFETY: the pointer is either null or the one leaked below, which
        // stays valid for the rest of the process. The swap means only the
        // first signal to arrive acts on it.
        unsafe { libc::unlink(name) };
    }
    // Then die of the signal that was sent rather than of anything this
    // handler decided, so a caller's shell reports the run the way it always
    // has, 130 for Ctrl-C, and a wait status that still says "interrupted".
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

#[cfg(unix)]
/// Points the handler at PATH and installs it for SPOOL_SIGNALS.
pub(crate) fn arm_spool_signals(path: &std::path::Path) {
    use std::os::unix::ffi::OsStrExt;
    use std::sync::atomic::Ordering;

    // Leaked deliberately: the handler may read it at any moment up to the
    // end of the process, so there is no later point at which freeing it
    // would be safe. One allocation, once, for the life of the run.
    let Ok(name) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
        return; // a NUL in a temporary path is not something to fail a run for
    };
    SPOOL_NAME.store(
        Box::leak(name.into_boxed_c_str()).as_ptr().cast_mut(),
        Ordering::SeqCst,
    );

    // Through a function pointer rather than straight from the function item,
    // which is the cast sighandler_t actually wants.
    let handler = remove_spool_on_signal as extern "C" fn(libc::c_int);
    for signal in SPOOL_SIGNALS {
        // SAFETY: the handler touches only an atomic and unlink(2).
        unsafe { libc::signal(signal, handler as libc::sighandler_t) };
    }
}

#[cfg(unix)]
/// Tells the handler there is no spool to remove any more.
pub(crate) fn disarm_spool_signals() {
    use std::sync::atomic::Ordering;
    SPOOL_NAME.store(std::ptr::null_mut(), Ordering::SeqCst);
}

/// Holds the spool signals off until the file exists and the handler knows its
/// name, restoring the mask on the way out.
///
/// The two steps cannot be done in one, and either order leaves a hole. Armed
/// after the file is created, a signal in between takes the
/// default action and leaves the copy behind, which is the one thing
/// --flexible says it does not do. Armed before, the handler is pointed at a
/// path this run has not created yet, and a signal there unlinks whatever
/// stands at that name: in a directory shared with every other account, that
/// is somebody else's file.
///
/// So neither. The signal is made to wait: blocked, it stays pending rather
/// than being lost, and is delivered the moment the mask is restored, by
/// which time the file is made and the handler knows it. Held for the two
/// steps only, never across the copy, which is the long part of the run and
/// the likeliest moment for a Ctrl-C to arrive.
#[cfg(unix)]
pub(crate) struct SpoolSignalsHeld(libc::sigset_t);

#[cfg(unix)]
impl SpoolSignalsHeld {
    /// Blocks SPOOL_SIGNALS until the value is dropped.
    pub(crate) fn new() -> Self {
        // SAFETY: sigemptyset initialises the set it is given, sigaddset adds
        // to an initialised one, and pthread_sigmask writes the old mask into
        // the second. Zeroed first so nothing is read uninitialised even if a
        // call were to fail.
        unsafe {
            let mut wanted: libc::sigset_t = std::mem::zeroed();
            let mut previous: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut wanted);
            for signal in SPOOL_SIGNALS {
                libc::sigaddset(&mut wanted, signal);
            }
            libc::pthread_sigmask(libc::SIG_BLOCK, &wanted, &mut previous);
            SpoolSignalsHeld(previous)
        }
    }
}

#[cfg(unix)]
impl Drop for SpoolSignalsHeld {
    fn drop(&mut self) {
        // SAFETY: puts back the mask new() took a copy of, and nothing else.
        unsafe {
            libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut());
        }
    }
}

/// How many names to try before giving up. Each carries this process's id and
/// a part nobody can predict, so a name is only ever taken by chance.
pub(crate) const SPOOL_ATTEMPTS: u32 = 8;

/// The name of the spool file for one attempt: the process's id, so a file
/// left behind can be traced to the run that left it, and 64 bits drawn from
/// the standard library's per-process random hash keys. Without that part the
/// names were the id and a count, which another account on the machine could
/// work out and create ahead of the run, all eight of them, to make
/// '--flexible=widest' fail.
pub(crate) fn spool_name(attempt: u32) -> String {
    use std::hash::BuildHasher;
    let unpredictable = std::collections::hash_map::RandomState::new()
        .hash_one((attempt, std::time::SystemTime::now()));
    format!("csvdt-widest-{}-{unpredictable:016x}.csv", std::process::id())
}

/// Creates the file the spool will be copied into.
///
/// The system temporary directory is shared with every other account on the
/// machine, so two things matter beyond simply opening a file:
///
///   * create_new, so an existing path is refused rather than written through.
///     Plain create follows a symlink left at the name csvdt is going to use,
///     which would have csvdt overwrite whatever the symlink points at, and
///     drop would then remove the symlink behind it.
///   * mode 0600 on Unix, because the default 0644 would publish the whole
///     input to every account on the machine, and this input is often a log.
pub(crate) fn create_spool_file(path: &std::path::Path) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options.open(path)
}

/// The environment variable that moves the temporary directory here.
///
/// Named rather than assumed, because the two families disagree and the advice
/// is worthless where it is wrong: std::env::temp_dir reads TMPDIR on Unix and
/// TMP then TEMP on Windows, so a Windows reader told to "set TMPDIR" would set
/// something nothing reads and get the same error again. The
/// remaining platforms fall back to the Unix spelling, which is what
/// temp_dir's own fallback path does.
#[cfg(windows)]
pub(crate) const TEMP_DIRECTORY_VARIABLE: &str = "TMP or TEMP";
/// The same, everywhere else.
#[cfg(not(windows))]
pub(crate) const TEMP_DIRECTORY_VARIABLE: &str = "TMPDIR";

/// Copies a reader to a temporary file so it can be read twice.
/// --flexible=widest has to count before it pads, and neither standard input
/// nor a pipe named as a file can be rewound.
pub(crate) fn spool(mut input: impl std::io::Read) -> Result<TempSpool, Box<dyn Error>> {
    let directory = std::env::temp_dir();
    let mut refusal = None;

    for attempt in 1..=SPOOL_ATTEMPTS {
        // A taken name costs a second try rather than the run.
        let path = directory.join(spool_name(attempt));

        // Held across creating the file and arming the handler, which are one
        // step as far as a signal is concerned; see SpoolSignalsHeld. A name
        // already taken arms nothing, so this simply lapses and the next name
        // is tried with the mask back as it was.
        #[cfg(unix)]
        let held = SpoolSignalsHeld::new();

        match create_spool_file(&path) {
            Ok(mut file) => {
                // Owned before the copy, so a copy that fails still cleans up.
                let spool = TempSpool { path };
                // Armed before it too, and before the first byte is written:
                // the copy is the long part of the run and so the likeliest
                // moment for a Ctrl-C to arrive.
                #[cfg(unix)]
                arm_spool_signals(&spool.path);
                // Only now may one arrive. Anything sent while the file was
                // being made was held rather than lost, and lands here.
                #[cfg(unix)]
                drop(held);
                // Both failures here would surface as a bare OS error, "No such
                // file or directory (os error 2)" with no path, no TMPDIR and
                // no mention of the spool, which open_input exists to avoid.
                if let Err(error) = std::io::copy(&mut input, &mut file) {
                    return Err(format!(
                        "could not copy the input to '{}', which \
                         '--flexible=widest' reads twice: {}",
                        spool.path.display(), error).into());
                }
                return Ok(spool);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                refusal = Some(error);
            }
            Err(error) => return Err(format!(
                "could not create a temporary file for '--flexible=widest' \
                 in {}: {}. That mode reads its input twice, and a pipe \
                 cannot be rewound, so it needs one. Set {} to a \
                 directory csvdt can write.",
                directory.display(), error, TEMP_DIRECTORY_VARIABLE).into()),
        }
    }

    Err(format!(
        "could not create a temporary file for '--flexible=widest' in {}: \
         every name csvdt tried was already taken ({}). That mode reads its \
         input twice, and a pipe cannot be rewound, so it needs one. Set {} \
         to a directory csvdt can write, or clear the stale 'csvdt-widest-*' \
         files out of that one.",
        directory.display(),
        refusal
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no reason given".to_string()),
        TEMP_DIRECTORY_VARIABLE,
    )
    .into())
}
