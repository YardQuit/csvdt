// What differs between platforms: probing the standard streams, asking a
// file for its identity, and refusing an input that is also the output. Each
// function has one signature, with the platform variants chosen here so the
// run logic elsewhere has no cfg arms.

use std::error::Error;

// Refuses a standard output that cannot take a single byte, before the run
// that would write to it begins.
//
// std answers Ok to a write on a standard stream that failed with EBADF: a
// process without usable standard output is one whose output goes nowhere,
// deliberately, and not an error std will surface. So `csvdt in.csv 1<out`,
// '<' where '>' was meant, converted the whole file, wrote none of it,
// counted what it had written on stderr, and exited 0. Nothing later in the
// run can notice, because the flush that should have failed returns Ok too.
//
// The check is a dup of the descriptor and a write of nothing to it. A
// zero-length write puts no byte anywhere and touches no reader (Linux
// answers a null write on a pipe before it looks for one), while still
// going through the kernel's check that the descriptor exists and was opened
// for writing.
//
// Only EBADF is refused here. A probe on a full disk answers ENOSPC, and
// that is the run's own failure to report, with its own message, at the
// point it actually runs out of room.
//
// Unix only: this is a gap std leaves on every platform, but the descriptor
// is the part of it there is a way to ask about.
pub(crate) fn require_writable_stdout() -> Result<(), Box<dyn Error>> {
    use std::io::Write;
    refuse_closed_stream(
        duplicate(std::io::stdout()),
        |stream| stream.write(&[]),
        "standard output cannot be written to. It is usually '<' where '>' \
         was meant: the output would have gone nowhere and the run would have \
         reported success",
    )
}

// The same gap on the way in. std hides EBADF on a read as well, and hides it
// as end of input, which is a thing an input is allowed to be: `csvdt -H -r0
// 0>out.csv`, '>' where '<' was meant, which has the shell empty out.csv on
// the way past, read nothing, wrote nothing, and exited 0, exactly as a run
// over an empty file does.
//
// A zero-length read consumes no byte and does not wait: a pipe with no
// writer yet answers it at once, as does a terminal.
pub(crate) fn require_readable_stdin() -> Result<(), Box<dyn Error>> {
    use std::io::Read;
    refuse_closed_stream(
        duplicate(std::io::stdin()),
        |stream| stream.read(&mut []),
        "standard input cannot be read from. It is usually '>' where '<' was \
         meant, which has the shell empty the file on the way past: the run \
         would have read nothing and reported success, which is what an empty \
         input does",
    )
}

// Refuses with MESSAGE a standard stream that PROBE finds closed, which on
// Unix is EBADF. Elsewhere there is no such answer to look for, and nothing
// is refused.
fn refuse_closed_stream(
    stream: Option<std::fs::File>,
    probe: impl FnOnce(&mut std::fs::File) -> std::io::Result<usize>,
    message: &str,
) -> Result<(), Box<dyn Error>> {
    #[cfg(unix)]
    let closed = |error: &std::io::Error| error.raw_os_error() == Some(libc::EBADF);
    #[cfg(not(unix))]
    let closed = |_: &std::io::Error| false;
    // A descriptor that cannot even be duplicated is closed too.
    let probed = stream
        .ok_or_else(|| std::io::Error::from_raw_os_error(EBADF))
        .and_then(|mut stream| probe(&mut stream));
    match probed {
        Err(error) if closed(&error) => Err(Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidInput, message.to_string()))),
        _ => Ok(()),
    }
}

// The error a closed descriptor gives, where the platform has one to give.
#[cfg(unix)]
const EBADF: i32 = libc::EBADF;
#[cfg(not(unix))]
const EBADF: i32 = 0;

// A File of its own on one of the run's standard streams, so it can be
// asked what a file is asked without closing the stream when it is dropped.
// None where the platform cannot duplicate it.
#[cfg(unix)]
fn duplicate(stream: impl std::os::fd::AsFd) -> Option<std::fs::File> {
    stream.as_fd().try_clone_to_owned().ok().map(std::fs::File::from)
}

#[cfg(windows)]
fn duplicate(stream: impl std::os::windows::io::AsHandle) -> Option<std::fs::File> {
    stream.as_handle().try_clone_to_owned().ok().map(std::fs::File::from)
}

#[cfg(not(any(unix, windows)))]
fn duplicate<T>(_stream: T) -> Option<std::fs::File> {
    None
}

// The identity of an open file as the platform tells it: device and inode on
// Unix, volume serial number and file index on Windows. Two handles reporting
// one identity are one file however it was spelled, through a symbolic
// link, a hard link, a second spelling of a directory, or '/dev/stdout'.
//
// None for a console or a device, where sharing is harmless: '/dev/null'
// twice over has nothing to lose. A regular file has one everywhere, and on
// Unix so does a fifo or a pipe, for the reason given below; on Windows a
// pipe has none.
#[cfg(unix)]
pub(crate) fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    metadata_identity(&file.metadata().ok()?)
}

// The identity file_identity reads, from what the platform says about a file.
#[cfg(unix)]
fn metadata_identity(about: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    // A regular file has data to lose; a fifo has a reader to mislead, which
    // is the same harm by another route: '--fill-log /dev/stdout' into a
    // pipe would put the log's own rows into the CSV somebody is reading,
    // the corruption the option exists to keep out of the output. A character
    // device is neither: '/dev/null' twice over is one identity with nothing
    // to lose and nobody to mislead, and refusing it would be refusing a run
    // that is fine.
    if !(about.file_type().is_file() || about.file_type().is_fifo()) {
        return None;
    }
    Some((about.dev(), about.ino()))
}

#[cfg(windows)]
pub(crate) fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    if !file.metadata().ok()?.file_type().is_file() {
        return None;
    }
    let mut about = unsafe { std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    // SAFETY: the handle is borrowed from a live File, and the struct is
    // owned here and correctly sized for the call.
    let answered = unsafe {
        GetFileInformationByHandle(file.as_raw_handle() as _, &mut about)
    };
    if answered == 0 {
        return None;
    }
    Some((
        u64::from(about.dwVolumeSerialNumber),
        (u64::from(about.nFileIndexHigh) << 32) | u64::from(about.nFileIndexLow),
    ))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn file_identity(_file: &std::fs::File) -> Option<(u64, u64)> {
    None
}

// The identity of the file a path names, following symbolic links, asked
// without opening it on Unix. Opening is not a question there: a fifo opened
// for reading waits for a writer, and when csvdt is to be that writer it
// waits for ever, and a pipe opened and closed again can leave its writer
// with nobody to write to.
#[cfg(unix)]
pub(crate) fn path_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    metadata_identity(&std::fs::metadata(path).ok()?)
}

// Windows has no fifo at a path, and its identity needs a handle, so the
// file is opened to ask.
#[cfg(not(unix))]
pub(crate) fn path_identity(path: &std::path::Path) -> Option<(u64, u64)> {
    file_identity(&std::fs::File::open(path).ok()?)
}

// The identity of standard input, where the platform can say.
pub(crate) fn stdin_identity() -> Option<(u64, u64)> {
    file_identity(&duplicate(std::io::stdin())?)
}

// The identity of standard output, where the platform can say.
pub(crate) fn stdout_identity() -> Option<(u64, u64)> {
    file_identity(&duplicate(std::io::stdout())?)
}

// Refuses an input file that is also where the output is going.
//
// The shell opens a redirection before csvdt is started, so `csvdt in.csv >
// in.csv` hands over a file already truncated to nothing: csvdt would read an
// empty file, write nothing and exit 0, and the only sign anything had
// happened would be that the data was gone. `csvdt in.csv >> in.csv` is the
// other half: the file is still whole when this is asked, and left to run it
// would produce the input followed by a conversion of the input, also at 0.
// cmd and PowerShell open '> in.csv' before csvdt starts just as a Unix shell
// does, so the guard holds on Windows too.
//
// Compared by identity rather than by path, so a symbolic link, a hard link,
// a junction, or a second spelling of the same directory is caught with it.
//
// Regular files only. Sharing a character device is meaningless rather than
// damaging: `csvdt /dev/null > /dev/null` is one inode twice over and has
// nothing to lose, and a run cannot be both ends of a pipe.
pub(crate) fn refuse_input_that_is_also_the_output(
    path: &std::path::Path,
    input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    let regular = input.metadata().is_ok_and(|about| about.file_type().is_file());
    let read_from = file_identity(input);
    if !regular || read_from.is_none() || read_from != stdout_identity() {
        return Ok(());
    }
    let path = path.display();
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("'{path}' is both the input and where the output is going. \
                 The shell opens a redirection before csvdt runs, so '> \
                 {path}' has already emptied it and '>> {path}' would leave \
                 the file followed by a conversion of itself. Write another \
                 file and move it into place"))))
}
