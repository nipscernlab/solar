//! What the operating system calls itself, read without starting a program.
//!
//! Each system keeps this somewhere different, and none of the three needs a child
//! process to answer:
//!
//! - **Linux** and the other systems that follow the os-release specification: the file
//!   `/etc/os-release`, and `/usr/lib/os-release` when the first is absent, which is the
//!   fallback the specification names.
//! - **macOS**: `/System/Library/CoreServices/SystemVersion.plist`, for `ProductVersion`
//!   and `ProductBuildVersion`.
//! - **Windows**: `RtlGetVersion` in `ntdll`, which reports the real version. Its
//!   documented alternative, `GetVersionEx`, lies to a program that carries no compatibility
//!   manifest, and would have this build call Windows 11 something else.
//!
//! When the source is missing or unreadable, this says why in one sentence and the caller
//! turns that into a warning. It never guesses.

/// What the operating system says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Release {
    /// The name of the system or the distribution, `null` when it does not give one.
    pub name: Option<String>,
    /// The release, as the system spells it.
    pub release: Option<String>,
    /// The build, for the systems that count builds separately from releases.
    pub build: Option<String>,
}

/// Reads what this system says about itself.
///
/// # Errors
///
/// Returns one sentence saying why nothing could be read, which becomes the message of an
/// `OS_RELEASE_UNAVAILABLE` warning. The call it belongs to still succeeds.
pub fn read() -> Result<Release, String> {
    read_here()
}

#[cfg(target_os = "linux")]
fn read_here() -> Result<Release, String> {
    read_os_release_file()
}

#[cfg(target_os = "macos")]
fn read_here() -> Result<Release, String> {
    const PATH: &str = "/System/Library/CoreServices/SystemVersion.plist";
    let text = std::fs::read_to_string(PATH)
        .map_err(|failure| format!("{PATH} could not be read: {failure}"))?;
    let release = plist_string(&text, "ProductVersion");
    let build = plist_string(&text, "ProductBuildVersion");
    if release.is_none() && build.is_none() {
        return Err(format!(
            "{PATH} was read and holds no ProductVersion this parser recognises; it may be \
             a binary property list"
        ));
    }
    Ok(Release {
        name: plist_string(&text, "ProductName").or_else(|| Some("macOS".to_owned())),
        release,
        build,
    })
}

#[cfg(target_os = "windows")]
fn read_here() -> Result<Release, String> {
    let (major, minor, build) = windows_version()?;
    Ok(Release {
        name: Some("Windows".to_owned()),
        release: Some(format!("{major}.{minor}.{build}")),
        build: Some(build.to_string()),
    })
}

/// Every other system: try the os-release specification, which several of them follow.
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn read_here() -> Result<Release, String> {
    read_os_release_file()
}

/// Where the os-release specification says to look, in the order it gives.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const OS_RELEASE_PATHS: [&str; 2] = ["/etc/os-release", "/usr/lib/os-release"];

/// Reads the first os-release file that exists, in the order the specification gives.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn read_os_release_file() -> Result<Release, String> {
    first_os_release(&OS_RELEASE_PATHS, |path| std::fs::read_to_string(path))
}

/// The same, with the reading passed in, so that the rule can be tested anywhere.
///
/// The paths are only there on Linux, and the code above them is only compiled there, so
/// without this split the rule for choosing between two files would be judged by no test
/// on Windows or on a Mac and by no mutation run except the weekly one. What is
/// conditional now is the list of paths; what decides is not.
#[allow(
    dead_code,
    reason = "only Linux and the systems that follow the os-release specification call               this, and the point of splitting it out is that the tests call it on every               system, including the ones where nothing else does"
)]
fn first_os_release(
    paths: &[&str],
    read: impl Fn(&str) -> std::io::Result<String>,
) -> Result<Release, String> {
    let mut last = String::new();
    for path in paths {
        match read(path) {
            Ok(text) => {
                let release = parse_os_release(&text);
                if release == Release::default() {
                    return Err(format!(
                        "{path} was read and holds none of NAME, VERSION_ID or ID"
                    ));
                }
                return Ok(release);
            }
            Err(failure) => last = format!("{path}: {failure}"),
        }
    }
    Err(format!(
        "no os-release file could be read, the last one tried being {last}"
    ))
}

/// The version Windows really is.
///
/// `RtlGetVersion` is the only way to ask without starting a program and without being
/// told a compatibility lie. The call is here, in six lines, and nowhere else.
#[cfg(target_os = "windows")]
fn windows_version() -> Result<(u32, u32, u32), String> {
    /// `RTL_OSVERSIONINFOW`, exactly as `ntdll` expects to receive it.
    #[repr(C)]
    struct OsVersionInfoW {
        size: u32,
        major: u32,
        minor: u32,
        build: u32,
        platform_id: u32,
        service_pack: [u16; 128],
    }

    #[allow(unsafe_code, reason = "a foreign function has no safe declaration")]
    #[link(name = "ntdll")]
    unsafe extern "system" {
        /// Fills in the version of the running system. Returns `STATUS_SUCCESS`, zero, or
        /// an `NTSTATUS` that is not zero.
        fn RtlGetVersion(info: *mut OsVersionInfoW) -> i32;
    }

    let mut info = OsVersionInfoW {
        size: u32::try_from(size_of::<OsVersionInfoW>()).unwrap_or(0),
        major: 0,
        minor: 0,
        build: 0,
        platform_id: 0,
        service_pack: [0; 128],
    };

    #[allow(
        unsafe_code,
        reason = "the one foreign call of the repository; the SAFETY argument is below"
    )]
    // SAFETY: `info` is a live, correctly aligned `RTL_OSVERSIONINFOW` whose `size` member
    // says how long it is, which is the whole of what `RtlGetVersion` requires. It writes
    // only inside that structure and returns a status rather than allocating anything, so
    // there is nothing to free and nothing else to get wrong.
    let status = unsafe { RtlGetVersion(&raw mut info) };

    if status == 0 {
        Ok((info.major, info.minor, info.build))
    } else {
        Err(format!("RtlGetVersion returned the status {status:#010x}"))
    }
}

/// Reads the members SOLAR reports out of an os-release file.
///
/// The format is one `KEY=value` per line, with `#` comments and shell style quoting, as
/// the os-release specification describes. Members that are not reported are skipped
/// rather than refused, because a distribution may add its own.
#[must_use]
pub fn parse_os_release(text: &str) -> Release {
    let (mut name, mut pretty, mut id) = (None, None, None);
    let (mut version_id, mut version, mut build) = (None, None, None);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, raw)) = line.split_once('=') else {
            continue;
        };
        let value = unquote(raw.trim());
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "NAME" => name = Some(value),
            "PRETTY_NAME" => pretty = Some(value),
            "ID" => id = Some(value),
            "VERSION_ID" => version_id = Some(value),
            "VERSION" => version = Some(value),
            "BUILD_ID" => build = Some(value),
            _ => {}
        }
    }

    Release {
        name: name.or(pretty).or(id),
        release: version_id.or(version),
        build,
    }
}

/// Removes shell quoting from an os-release value.
fn unquote(raw: &str) -> String {
    let mut characters = raw.chars();
    let (first, last) = (characters.next(), characters.next_back());
    match (first, last) {
        (Some('"'), Some('"')) => unescape(characters.as_str()),
        (Some('\''), Some('\'')) => characters.as_str().to_owned(),
        _ => raw.to_owned(),
    }
}

/// Undoes the four escapes the os-release specification allows inside double quotes.
fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some(escaped @ ('"' | '\\' | '$' | '`')) => out.push(escaped),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// The string that follows a key in an XML property list.
///
/// Only the shape `SystemVersion.plist` has is understood: a flat dictionary of keys and
/// strings. A binary property list yields `None`, and the caller reports that it could not
/// read the release rather than inventing one.
#[must_use]
pub fn plist_string(text: &str, key: &str) -> Option<String> {
    let after = text.split_once(&format!("<key>{key}</key>"))?.1;
    let opened = after.find("<string>")? + "<string>".len();
    let rest = after.get(opened..)?;
    let closed = rest.find("</string>")?;
    let value = rest.get(..closed)?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file Ubuntu 24.04 ships, trimmed to the members that are read.
    const UBUNTU: &str = r#"PRETTY_NAME="Ubuntu 24.04.1 LTS"
NAME="Ubuntu"
VERSION_ID="24.04"
VERSION="24.04.1 LTS (Noble Numbat)"
VERSION_CODENAME=noble
ID=ubuntu
ID_LIKE=debian
HOME_URL="https://www.ubuntu.com/"
UBUNTU_CODENAME=noble
"#;

    /// The shape Arch ships: no version at all, because it is rolling.
    const ARCH: &str = r#"NAME="Arch Linux"
PRETTY_NAME="Arch Linux"
ID=arch
BUILD_ID=rolling
ANSI_COLOR="38;2;23;147;209"
"#;

    #[test]
    fn the_first_readable_os_release_file_is_the_one_used() {
        let reader = |path: &str| {
            if path == "/second" {
                Ok(UBUNTU.to_owned())
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "no such file",
                ))
            }
        };
        let release =
            first_os_release(&["/first", "/second"], reader).expect("the second file is readable");
        assert_eq!(release.name.as_deref(), Some("Ubuntu"));
    }

    #[test]
    fn the_order_of_the_paths_is_the_order_of_the_specification() {
        // Both readable, and the first one wins: `/etc/os-release` overrides the
        // fallback in `/usr/lib`, which is what the specification requires.
        let reader = |path: &str| {
            Ok(if path == "/first" {
                "NAME=\"First\"\n".to_owned()
            } else {
                "NAME=\"Second\"\n".to_owned()
            })
        };
        let release = first_os_release(&["/first", "/second"], reader).expect("both are readable");
        assert_eq!(release.name.as_deref(), Some("First"));
    }

    #[test]
    fn a_file_that_says_nothing_is_an_error_rather_than_an_empty_release() {
        // Read, and holding none of the members that matter: reporting an empty release
        // would look like an answer. The fallback is not tried, because the file that
        // should have said was there and did not.
        let reader = |_: &str| Ok("# nothing but a comment\n".to_owned());
        let failure = first_os_release(&["/first", "/second"], reader)
            .expect_err("a file that says nothing is not an answer");
        assert!(failure.contains("/first"), "{failure}");
        assert!(failure.contains("NAME"), "{failure}");
    }

    #[test]
    fn no_readable_file_names_the_last_one_tried() {
        let reader = |_: &str| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "denied",
            ))
        };
        let failure =
            first_os_release(&["/first", "/second"], reader).expect_err("nothing is readable");
        assert!(failure.contains("/second"), "the last one tried: {failure}");
        assert!(failure.contains("denied"), "and why: {failure}");
    }

    #[test]
    fn no_paths_at_all_is_an_error_and_not_a_panic() {
        let reader = |_: &str| Ok(String::new());
        assert!(first_os_release(&[], reader).is_err());
    }

    #[test]
    fn a_real_os_release_file_is_read() {
        let release = parse_os_release(UBUNTU);
        assert_eq!(release.name.as_deref(), Some("Ubuntu"));
        assert_eq!(release.release.as_deref(), Some("24.04"));
        assert_eq!(release.build, None);
    }

    #[test]
    fn a_rolling_distribution_has_a_build_and_no_release() {
        let release = parse_os_release(ARCH);
        assert_eq!(release.name.as_deref(), Some("Arch Linux"));
        assert_eq!(release.release, None);
        assert_eq!(release.build.as_deref(), Some("rolling"));
    }

    #[test]
    fn comments_blank_lines_and_unknown_members_are_skipped() {
        let text = "# a comment\n\n  \nNOPE=1\nNAME=Void\nnot an assignment\n";
        let release = parse_os_release(text);
        assert_eq!(release.name.as_deref(), Some("Void"));
    }

    #[test]
    fn the_name_falls_back_the_way_the_specification_orders_it() {
        assert_eq!(
            parse_os_release("PRETTY_NAME=\"P\"\nID=i\n")
                .name
                .as_deref(),
            Some("P")
        );
        assert_eq!(parse_os_release("ID=i\n").name.as_deref(), Some("i"));
        assert_eq!(
            parse_os_release("VERSION=\"9 (x)\"\n").release.as_deref(),
            Some("9 (x)")
        );
        assert_eq!(parse_os_release("").name, None);
    }

    #[test]
    fn quoting_is_undone_the_way_a_shell_would() {
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("\"quoted\""), "quoted");
        assert_eq!(unquote("'single'"), "single");
        assert_eq!(unquote("\"a \\\"b\\\" c\""), "a \"b\" c");
        assert_eq!(unquote("\"back\\\\slash\""), "back\\slash");
        assert_eq!(
            unquote("\"a \\z b\""),
            "a \\z b",
            "an escape that is not one is kept"
        );
        assert_eq!(unquote("\""), "\"");
    }

    /// The shape of the file macOS keeps, with the members that are read.
    const SYSTEM_VERSION: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
	<key>ProductBuildVersion</key>
	<string>24A335</string>
	<key>ProductName</key>
	<string>macOS</string>
	<key>ProductUserVisibleVersion</key>
	<string>15.0</string>
	<key>ProductVersion</key>
	<string>15.0</string>
</dict>
</plist>
"#;

    #[test]
    fn a_property_list_gives_up_its_version_and_its_build() {
        assert_eq!(
            plist_string(SYSTEM_VERSION, "ProductVersion").as_deref(),
            Some("15.0")
        );
        assert_eq!(
            plist_string(SYSTEM_VERSION, "ProductBuildVersion").as_deref(),
            Some("24A335")
        );
        assert_eq!(
            plist_string(SYSTEM_VERSION, "ProductName").as_deref(),
            Some("macOS")
        );
        assert_eq!(plist_string(SYSTEM_VERSION, "NotThere"), None);
    }

    #[test]
    fn a_property_list_that_is_not_xml_yields_nothing_rather_than_nonsense() {
        assert_eq!(
            plist_string("bplist00\u{0}\u{1}rubbish", "ProductVersion"),
            None
        );
        assert_eq!(
            plist_string("<key>ProductVersion</key>", "ProductVersion"),
            None
        );
        assert_eq!(plist_string("<key>A</key><string>  </string>", "A"), None);
    }

    #[test]
    fn this_machine_reports_its_own_release() {
        // Every system the workspace supports can answer this. A failure here on one of
        // them is a real gap, not a quirk of the machine running the test.
        let release = read().unwrap_or_else(|why| panic!("the release could not be read: {why}"));
        assert!(release.name.is_some(), "no name came back: {release:?}");
        assert!(
            release.release.is_some() || release.build.is_some(),
            "neither a release nor a build came back: {release:?}"
        );

        if cfg!(target_os = "windows") {
            let reported = release.release.unwrap_or_default();
            assert!(
                reported.starts_with("10.0."),
                "Windows reports itself as {reported}"
            );
            assert_eq!(release.name.as_deref(), Some("Windows"));
        }
    }
}
