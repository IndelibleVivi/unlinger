use std::ffi::{CStr, c_void};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path};
use std::ptr;
use unlinger_core::AppBundleVersion;

const MAX_APP_ANCESTORS: usize = 8;
const MAX_INFO_PLIST_BYTES: u64 = 256 * 1024;
const MAX_INFO_FIELD_BYTES: usize = 256;
const UTF8_ENCODING: u32 = 0x0800_0100;

type CfTypeRef = *const c_void;
type CfAllocatorRef = *const c_void;
type CfDataRef = *const c_void;
type CfPropertyListRef = *const c_void;
type CfDictionaryRef = *const c_void;
type CfStringRef = *const c_void;
type CfErrorRef = *const c_void;
type CfTypeId = libc::c_ulong;
type CfOptionFlags = libc::c_ulong;
type CfIndex = libc::c_long;

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFDataCreate(allocator: CfAllocatorRef, bytes: *const u8, length: CfIndex) -> CfDataRef;
    fn CFPropertyListCreateWithData(
        allocator: CfAllocatorRef,
        data: CfDataRef,
        options: CfOptionFlags,
        format: *mut CfIndex,
        error: *mut CfErrorRef,
    ) -> CfPropertyListRef;
    fn CFDictionaryGetTypeID() -> CfTypeId;
    fn CFStringGetTypeID() -> CfTypeId;
    fn CFGetTypeID(value: CfTypeRef) -> CfTypeId;
    fn CFDictionaryGetValue(dictionary: CfDictionaryRef, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        allocator: CfAllocatorRef,
        value: *const libc::c_char,
        encoding: u32,
    ) -> CfStringRef;
    fn CFStringGetLength(value: CfStringRef) -> CfIndex;
    fn CFStringGetMaximumSizeForEncoding(length: CfIndex, encoding: u32) -> CfIndex;
    fn CFStringGetCString(
        value: CfStringRef,
        buffer: *mut libc::c_char,
        buffer_size: CfIndex,
        encoding: u32,
    ) -> u8;
    fn CFRelease(value: CfTypeRef);
}

struct CfOwned(CfTypeRef);

impl CfOwned {
    fn new(value: CfTypeRef) -> Option<Self> {
        (!value.is_null()).then(|| Self(value))
    }
}

impl Drop for CfOwned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

pub(super) fn collect_app_bundle_version(executable: &Path) -> Option<AppBundleVersion> {
    if !executable.is_absolute() || !is_exact_bundle_executable(executable) {
        return None;
    }
    let executable_file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
        .open(executable)
        .ok()?;
    if !executable_file.metadata().ok()?.file_type().is_file() {
        return None;
    }

    let bundle = executable
        .ancestors()
        .take(MAX_APP_ANCESTORS)
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".app"))
        })?;
    let info_plist = bundle.join("Contents/Info.plist");
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW_ANY | libc::O_CLOEXEC)
        .open(info_plist)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.file_type().is_file()
        || metadata.nlink() != 1
        || metadata.len() == 0
        || metadata.len() > MAX_INFO_PLIST_BYTES
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).ok()?);
    file.by_ref()
        .take(MAX_INFO_PLIST_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if u64::try_from(bytes.len()).ok()? != metadata.len() {
        return None;
    }
    parse_info_plist(&bytes)
}

fn is_exact_bundle_executable(executable: &Path) -> bool {
    let Some(bundle) = executable
        .ancestors()
        .take(MAX_APP_ANCESTORS)
        .find(|ancestor| {
            ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".app"))
        })
    else {
        return false;
    };
    let Ok(relative) = executable.strip_prefix(bundle) else {
        return false;
    };
    let mut components = relative.components();
    matches!(components.next(), Some(Component::Normal(value)) if value == "Contents")
        && matches!(components.next(), Some(Component::Normal(value)) if value == "MacOS")
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
}

fn parse_info_plist(bytes: &[u8]) -> Option<AppBundleVersion> {
    let length = CfIndex::try_from(bytes.len()).ok()?;
    let data = CfOwned::new(unsafe { CFDataCreate(ptr::null(), bytes.as_ptr(), length) })?;
    let mut error: CfErrorRef = ptr::null();
    let property_list = unsafe {
        CFPropertyListCreateWithData(ptr::null(), data.0, 0, ptr::null_mut(), &raw mut error)
    };
    let _error = CfOwned::new(error);
    let property_list = CfOwned::new(property_list)?;
    if unsafe { CFGetTypeID(property_list.0) } != unsafe { CFDictionaryGetTypeID() } {
        return None;
    }
    let dictionary = property_list.0.cast();
    let bundle_id = dictionary_string(dictionary, c"CFBundleIdentifier")?;
    let short_version = dictionary_string(dictionary, c"CFBundleShortVersionString")?;
    Some(AppBundleVersion {
        bundle_id,
        short_version,
    })
}

fn dictionary_string(dictionary: CfDictionaryRef, key: &CStr) -> Option<String> {
    let key = CfOwned::new(unsafe {
        CFStringCreateWithCString(ptr::null(), key.as_ptr(), UTF8_ENCODING)
    })?;
    let value = unsafe { CFDictionaryGetValue(dictionary, key.0) };
    if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
        return None;
    }
    let length = unsafe { CFStringGetLength(value) };
    if length <= 0 {
        return None;
    }
    let maximum = unsafe { CFStringGetMaximumSizeForEncoding(length, UTF8_ENCODING) };
    let maximum = usize::try_from(maximum).ok()?;
    if maximum == 0 || maximum > MAX_INFO_FIELD_BYTES {
        return None;
    }
    let mut buffer = vec![0_i8; maximum.checked_add(1)?];
    let buffer_size = CfIndex::try_from(buffer.len()).ok()?;
    if unsafe { CFStringGetCString(value, buffer.as_mut_ptr(), buffer_size, UTF8_ENCODING) } == 0 {
        return None;
    }
    let value = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_str().ok()?;
    (!value.trim().is_empty()).then(|| value.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempBundle {
        root: PathBuf,
        executable: PathBuf,
        info_plist: PathBuf,
    }

    impl TempBundle {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("wall clock")
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "unlinger-version-{label}-{}-{nonce}",
                std::process::id()
            ));
            let contents = root.join("Synthetic Browser.app/Contents");
            let executable = contents.join("MacOS/Synthetic Browser");
            fs::create_dir_all(executable.parent().expect("executable parent"))
                .expect("create synthetic app bundle");
            let root = fs::canonicalize(root).expect("canonical synthetic app root");
            let contents = root.join("Synthetic Browser.app/Contents");
            let executable = contents.join("MacOS/Synthetic Browser");
            let info_plist = contents.join("Info.plist");
            fs::write(&executable, b"synthetic executable").expect("write executable");
            Self {
                root,
                executable,
                info_plist,
            }
        }

        fn write_plist(&self) {
            fs::write(
                &self.info_plist,
                br#"<?xml version="1.0" encoding="UTF-8"?>
                    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
                    <plist version="1.0"><dict>
                    <key>CFBundleIdentifier</key><string>com.google.chrome.for.testing</string>
                    <key>CFBundleShortVersionString</key><string>151.0.7922.34</string>
                    </dict></plist>"#,
            )
            .expect("write Info.plist");
        }
    }

    impl Drop for TempBundle {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn reads_exact_bundle_id_and_short_version_from_the_executable_bundle() {
        let bundle = TempBundle::new("exact");
        bundle.write_plist();

        assert_eq!(
            collect_app_bundle_version(&bundle.executable),
            Some(AppBundleVersion {
                bundle_id: "com.google.chrome.for.testing".to_owned(),
                short_version: "151.0.7922.34".to_owned(),
            })
        );
    }

    #[test]
    fn rejects_symlinked_bundle_paths_and_info_plists() {
        let bundle = TempBundle::new("symlink");
        bundle.write_plist();
        let alias = bundle.root.join("Alias.app");
        symlink(bundle.root.join("Synthetic Browser.app"), &alias).expect("symlink app bundle");
        assert_eq!(
            collect_app_bundle_version(&alias.join("Contents/MacOS/Synthetic Browser")),
            None
        );

        fs::remove_file(&bundle.info_plist).expect("remove owned test plist");
        let real_plist = bundle.root.join("real.plist");
        fs::write(&real_plist, b"private test plist").expect("write real plist");
        symlink(&real_plist, &bundle.info_plist).expect("symlink Info.plist");
        assert_eq!(collect_app_bundle_version(&bundle.executable), None);
    }

    #[test]
    fn malformed_missing_and_oversized_plists_fail_closed() {
        let bundle = TempBundle::new("malformed");
        fs::write(&bundle.info_plist, b"not a plist").expect("write malformed plist");
        assert_eq!(collect_app_bundle_version(&bundle.executable), None);

        fs::write(
            &bundle.info_plist,
            br#"<?xml version="1.0"?><plist version="1.0"><dict>
                <key>CFBundleIdentifier</key><string>com.google.chrome.for.testing</string>
                </dict></plist>"#,
        )
        .expect("write missing-version plist");
        assert_eq!(collect_app_bundle_version(&bundle.executable), None);

        fs::write(
            &bundle.info_plist,
            vec![b'x'; usize::try_from(MAX_INFO_PLIST_BYTES + 1).expect("bounded test size")],
        )
        .expect("write oversized plist");
        assert_eq!(collect_app_bundle_version(&bundle.executable), None);
    }

    #[test]
    fn nearest_app_bundle_owns_a_nested_exact_executable() {
        let bundle = TempBundle::new("nearest");
        bundle.write_plist();
        let nested_contents = bundle
            .root
            .join("Synthetic Browser.app/Contents/Frameworks/Nested Helper.app/Contents");
        let nested_executable = nested_contents.join("MacOS/Nested Helper");
        fs::create_dir_all(
            nested_executable
                .parent()
                .expect("nested executable parent"),
        )
        .expect("create nested app");
        fs::write(&nested_executable, b"nested executable").expect("write nested executable");
        fs::write(
            nested_contents.join("Info.plist"),
            br#"<?xml version="1.0"?><plist version="1.0"><dict>
                <key>CFBundleIdentifier</key><string>com.example.nested-helper</string>
                <key>CFBundleShortVersionString</key><string>9.8.7</string>
                </dict></plist>"#,
        )
        .expect("write nested Info.plist");

        assert_eq!(
            collect_app_bundle_version(&nested_executable),
            Some(AppBundleVersion {
                bundle_id: "com.example.nested-helper".to_owned(),
                short_version: "9.8.7".to_owned(),
            })
        );
    }
}
