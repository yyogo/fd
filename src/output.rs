use std::borrow::Cow;
use std::io::{self, StdoutLock, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use lazy_static::lazy_static;
use lscolors::{Indicator, LsColors, Style};

use crate::config::Config;
use crate::entry::DirEntry;
use crate::error::print_error;
use crate::exit_codes::ExitCode;
use crate::filesystem::strip_current_dir;

lazy_static! {
    static ref MAIN_SEPARATOR_STR: String = std::path::MAIN_SEPARATOR.to_string();
}

fn replace_path_separator(path: &str, new_path_separator: &str) -> String {
    path.replace(std::path::MAIN_SEPARATOR, new_path_separator)
}

fn stripped_path(entry: &DirEntry) -> &Path {
    let path = entry.path();
    if path.is_absolute() {
        path
    } else {
        strip_current_dir(path)
    }
}

// TODO: this function is performance critical and can probably be optimized
pub fn print_entry(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
    wants_to_quit: &Arc<AtomicBool>,
) {
    let r = if let Some(ref ls_colors) = config.ls_colors {
        print_entry_colorized(stdout, entry, config, ls_colors, wants_to_quit)
    } else {
        print_entry_uncolorized(stdout, entry, config)
    };

    if let Err(e) = r {
        if e.kind() == ::std::io::ErrorKind::BrokenPipe {
            // Exit gracefully in case of a broken pipe (e.g. 'fd ... | head -n 3').
            ExitCode::Success.exit();
        } else {
            print_error(format!("Could not write to output: {}", e));
            ExitCode::GeneralError.exit();
        }
    }
}

// Display a trailing slash if the path is a directory and the config option is enabled.
// If the path_separator option is set, display that instead.
// The trailing slash will not be colored.
#[inline]
fn print_trailing_slash(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
    style: Option<&Style>,
) -> io::Result<()> {
    if entry.metadata().map_or(false, |m| m.is_dir()) {
        let separator = config
            .path_separator
            .as_ref()
            .unwrap_or(&MAIN_SEPARATOR_STR);
        write!(
            stdout,
            "{}",
            style
                .map(Style::to_ansi_term_style)
                .unwrap_or_default()
                .paint(separator)
        )?;
    }
    Ok(())
}

// TODO: this function is performance critical and can probably be optimized
fn print_entry_colorized(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
    ls_colors: &LsColors,
    wants_to_quit: &Arc<AtomicBool>,
) -> io::Result<()> {
    // Split the path between the parent and the last component
    let mut offset = 0;
    let path = stripped_path(entry);
    let path_str = path.to_string_lossy();

    if let Some(parent) = path.parent() {
        offset = parent.to_string_lossy().len();
        for c in path_str[offset..].chars() {
            if std::path::is_separator(c) {
                offset += c.len_utf8();
            } else {
                break;
            }
        }
    }

    if offset > 0 {
        let mut parent_str = Cow::from(&path_str[..offset]);
        if let Some(ref separator) = config.path_separator {
            *parent_str.to_mut() = replace_path_separator(&parent_str, separator);
        }

        let style = ls_colors
            .style_for_indicator(Indicator::Directory)
            .map(Style::to_ansi_term_style)
            .unwrap_or_default();
        write!(stdout, "{}", style.paint(parent_str))?;
    }

    let style = ls_colors
        .style_for_path_with_metadata(path, entry.metadata())
        .map(Style::to_ansi_term_style)
        .unwrap_or_default();
    write!(stdout, "{}", style.paint(&path_str[offset..]))?;

    print_trailing_slash(
        stdout,
        entry,
        config,
        ls_colors.style_for_indicator(Indicator::Directory),
    )?;

    if config.null_separator {
        write!(stdout, "\0")?;
    } else {
        writeln!(stdout)?;
    }

    if wants_to_quit.load(Ordering::Relaxed) {
        ExitCode::KilledBySigint.exit();
    }

    Ok(())
}

// TODO: this function is performance critical and can probably be optimized
fn print_entry_uncolorized_base(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
) -> io::Result<()> {
    let separator = if config.null_separator { "\0" } else { "\n" };
    let path = stripped_path(entry);

    let mut path_string = path.to_string_lossy();
    if let Some(ref separator) = config.path_separator {
        *path_string.to_mut() = replace_path_separator(&path_string, separator);
    }
    write!(stdout, "{}", path_string)?;
    print_trailing_slash(stdout, entry, config, None)?;
    write!(stdout, "{}", separator)
}

#[cfg(not(unix))]
fn print_entry_uncolorized(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
) -> io::Result<()> {
    print_entry_uncolorized_base(stdout, entry, config)
}

#[cfg(unix)]
fn print_entry_uncolorized(
    stdout: &mut StdoutLock,
    entry: &DirEntry,
    config: &Config,
) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    if config.interactive_terminal || config.path_separator.is_some() {
        // Fall back to the base implementation
        print_entry_uncolorized_base(stdout, entry, config)
    } else {
        // Print path as raw bytes, allowing invalid UTF-8 filenames to be passed to other processes
        let separator = if config.null_separator { b"\0" } else { b"\n" };
        stdout.write_all(stripped_path(entry).as_os_str().as_bytes())?;
        print_trailing_slash(stdout, entry, config, None)?;
        stdout.write_all(separator)
    }
}
