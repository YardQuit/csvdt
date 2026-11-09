// What differs between platforms: probing the standard streams, asking a
// file for its identity, and refusing an input that is also the output. Each
// function has one signature, with the platform variants chosen here so the
// run logic in main.rs has no cfg arms.

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
#[cfg(unix)]
pub(crate) fn require_writable_stdout() -> Result<(), Box<dyn Error>> {
    use std::io::Write;
    use std::os::fd::AsFd;

    let probed = std::io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|mut duplicate| duplicate.write(&[]));
    match probed {
        Err(error) if error.raw_os_error() == Some(libc::EBADF) => {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "standard output cannot be written to. It is usually '<' \
                 where '>' was meant: the output would have gone nowhere and \
                 the run would have reported success".to_string())))
        },
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
pub(crate) fn require_writable_stdout() -> Result<(), Box<dyn Error>> {
    Ok(())
}

// The same gap on the way in. std hides EBADF on a read as well, and hides it
// as end of input, which is a thing an input is allowed to be: `csvdt -H -r0
// 0>out.csv`, '>' where '<' was meant, which has the shell empty out.csv on
// the way past, read nothing, wrote nothing, and exited 0, exactly as a run
// over an empty file does.
//
// A zero-length read consumes no byte and does not wait: a pipe with no
// writer yet answers it at once, as does a terminal.
#[cfg(unix)]
pub(crate) fn require_readable_stdin() -> Result<(), Box<dyn Error>> {
    use std::io::Read;
    use std::os::fd::AsFd;

    let probed = std::io::stdin()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|mut duplicate| duplicate.read(&mut []));
    match probed {
        Err(error) if error.raw_os_error() == Some(libc::EBADF) => {
            Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "standard input cannot be read from. It is usually '>' where \
                 '<' was meant, which has the shell empty the file on the way \
                 past: the run would have read nothing and reported success, \
                 which is what an empty input does".to_string())))
        },
        _ => Ok(()),
    }
}

#[cfg(not(unix))]
pub(crate) fn require_readable_stdin() -> Result<(), Box<dyn Error>> {
    Ok(())
}

// The identity of an open file as the platform tells it: device and inode on
// Unix, volume serial number and file index on Windows. Two handles reporting
// one identity are one file however it was spelled, through a symbolic
// link, a hard link, a second spelling of a directory, or '/dev/stdout'.
//
// None where the question is meaningless rather than answerable: a pipe, a
// console, a device. Sharing one of those is not damaging the way sharing a
// regular file is, and a run cannot be both ends of a pipe.
#[cfg(unix)]
pub(crate) fn file_identity(file: &std::fs::File) -> Option<(u64, u64)> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    let about = file.metadata().ok()?;
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

// The identity of one of the run's own streams, standard input or standard
// output, which carry no path to open by.
#[cfg(unix)]
fn stream_identity(stream: std::os::fd::BorrowedFd) -> Option<(u64, u64)> {
    let duplicate = stream.try_clone_to_owned().ok().map(std::fs::File::from)?;
    file_identity(&duplicate)
}

// The identity of standard input, where the platform can say.
pub(crate) fn stdin_identity() -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        stream_identity(std::io::stdin().as_fd())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

// The identity of standard output, where the platform can say.
pub(crate) fn stdout_identity() -> Option<(u64, u64)> {
    #[cfg(unix)]
    {
        use std::os::fd::AsFd;
        stream_identity(std::io::stdout().as_fd())
    }
    #[cfg(not(unix))]
    {
        None
    }
}

// Refuses an input file that is also where the output is going.
//
// The shell opens a redirection before csvdt is started, so `csvdt in.csv >
// in.csv` hands over a file already truncated to nothing: csvdt would read an
// empty file, write nothing and exit 0, and the only sign anything had
// happened would be that the data was gone. `csvdt in.csv >> in.csv` is the
// other half: the file is still whole when this is asked, and left to run it
// would produce the input followed by a conversion of the input, also at 0.
//
// Compared by device and inode rather than by path, so a symbolic link, a
// hard link, or a second spelling of the same directory is caught with it.
//
// Regular files only. Sharing a character device is meaningless rather than
// damaging: `csvdt /dev/null > /dev/null` is one inode twice over and has
// nothing to lose, and a run cannot be both ends of a pipe.
#[cfg(unix)]
pub(crate) fn refuse_input_that_is_also_the_output(
    path: &str,
    input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    use std::os::fd::AsFd;
    use std::os::unix::fs::MetadataExt;

    let Ok(read_from) = input.metadata() else { return Ok(()) };
    if !read_from.file_type().is_file() {
        return Ok(());
    }
    let written_to = std::io::stdout()
        .as_fd()
        .try_clone_to_owned()
        .map(std::fs::File::from)
        .and_then(|duplicate| duplicate.metadata());
    let Ok(written_to) = written_to else { return Ok(()) };
    if written_to.dev() != read_from.dev() || written_to.ino() != read_from.ino() {
        return Ok(());
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("'{path}' is both the input and where the output is going. \
                 The shell opens a redirection before csvdt runs, so '> \
                 {path}' has already emptied it and '>> {path}' would leave \
                 the file followed by a conversion of itself. Write another \
                 file and move it into place"))))
}

// The same refusal on Windows, where a redirection destroys the file exactly
// as it does on Unix: cmd and PowerShell both open '> in.csv' before csvdt
// starts. Without this arm the guard would be Unix-only while KNOWN LIMITS
// promises it on every platform, so the tests for it run on Windows too.
//
// A file's identity here is its volume serial number and its 64-bit index,
// which is what dev and ino are on the other side. std has no way to ask:
// MetadataExt::file_index is still unstable, so the call is made directly.
// Being an identity rather than a path, it catches a junction, a hard link
// and a second spelling of the same file, as the Unix side does.
//
// The handles are borrowed rather than duplicated. The Unix side clones its
// descriptor because it needs a File to call metadata() on; nothing is opened
// or closed here, so there is no handle to lose and no risk of closing the
// one the run is about to write to.
#[cfg(windows)]
pub(crate) fn refuse_input_that_is_also_the_output(
    path: &str,
    input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    use std::os::windows::io::AsRawHandle;

    // None where the question cannot be answered, a pipe, a console, a
    // device, which is the same "then it is not the case" the Unix side
    // takes for a handle it cannot stat.
    fn identity(handle: std::os::windows::io::RawHandle) -> Option<(u32, u64)> {
        use windows_sys::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut about = unsafe {
            std::mem::zeroed::<BY_HANDLE_FILE_INFORMATION>()
        };
        // SAFETY: the handle is borrowed from a live File or from Stdout, and
        // the struct is owned here and correctly sized for the call.
        let answered = unsafe {
            GetFileInformationByHandle(handle as _, &mut about)
        };
        if answered == 0 {
            return None;
        }
        Some((
            about.dwVolumeSerialNumber,
            (u64::from(about.nFileIndexHigh) << 32)
                | u64::from(about.nFileIndexLow),
        ))
    }

    let Ok(read_from) = input.metadata() else { return Ok(()) };
    if !read_from.file_type().is_file() {
        return Ok(());
    }
    let Some(source) = identity(input.as_raw_handle()) else {
        return Ok(());
    };
    let Some(destination) = identity(std::io::stdout().as_raw_handle()) else {
        return Ok(());
    };
    if source != destination {
        return Ok(());
    }
    Err(Box::new(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("'{path}' is both the input and where the output is going. \
                 The shell opens a redirection before csvdt runs, so '> \
                 {path}' has already emptied it and '>> {path}' would leave \
                 the file followed by a conversion of itself. Write another \
                 file and move it into place"))))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn refuse_input_that_is_also_the_output(
    _path: &str,
    _input: &std::fs::File,
) -> Result<(), Box<dyn Error>> {
    Ok(())
}
