//! Parse already admitted configuration bytes without filesystem includes.
//!
//! The safe git2 facade has no memory-config constructor. This module uses only
//! libgit2's public config API, owns every returned allocation, and returns
//! owned strings. It never borrows a git2 handle or follows an include path.
#![allow(unsafe_code)]

use std::ffi::{CStr, c_char, c_int, c_uint};
use std::ptr::{self, NonNull};

use libgit2_sys as raw;

#[repr(C)]
struct MemoryOptions {
    version: c_uint,
    backend_type: *const c_char,
    origin_path: *const c_char,
}

// Public declaration from git2/sys/config.h in the locked libgit2 1.9.4.
// Passing null options selects the documented defaults.
unsafe extern "C" {
    fn git_config_backend_from_string(
        out: *mut *mut raw::git_config_backend,
        text: *const c_char,
        length: usize,
        options: *mut MemoryOptions,
    ) -> c_int;
}

struct Library;
impl Drop for Library {
    fn drop(&mut self) {
        // Balanced with the successful initialization in parse().
        unsafe {
            raw::git_libgit2_shutdown();
        }
    }
}

struct Backend(NonNull<raw::git_config_backend>);
impl Drop for Backend {
    fn drop(&mut self) {
        // The public backend owns its free function. Ownership transfers to
        // Config only after git_config_add_backend succeeds.
        if let Some(free) = unsafe { self.0.as_ref().free } {
            free(self.0.as_ptr());
        }
    }
}

struct Config(NonNull<raw::git_config>);
impl Drop for Config {
    fn drop(&mut self) {
        unsafe {
            raw::git_config_free(self.0.as_ptr());
        }
    }
}

struct IteratorHandle(NonNull<raw::git_config_iterator>);
impl Drop for IteratorHandle {
    fn drop(&mut self) {
        unsafe {
            raw::git_config_iterator_free(self.0.as_ptr());
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ConfigEntry {
    pub name: String,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ConfigRefusal {
    Invalid,
    CapacityExceeded,
}

pub(super) fn parse(bytes: &[u8], max_entries: usize) -> Result<Vec<ConfigEntry>, ConfigRefusal> {
    if bytes.contains(&0) {
        return Err(ConfigRefusal::Invalid);
    }
    if unsafe { raw::git_libgit2_init() } < 0 {
        return Err(ConfigRefusal::Invalid);
    }
    let _library = Library;
    let mut config = ptr::null_mut();
    if unsafe { raw::git_config_new(&mut config) } < 0 {
        return Err(ConfigRefusal::Invalid);
    }
    let config = Config(NonNull::new(config).ok_or(ConfigRefusal::Invalid)?);
    let mut backend = ptr::null_mut();
    if unsafe {
        git_config_backend_from_string(
            &mut backend,
            bytes.as_ptr().cast(),
            bytes.len(),
            ptr::null_mut(),
        )
    } < 0
    {
        return Err(ConfigRefusal::Invalid);
    }
    let backend = Backend(NonNull::new(backend).ok_or(ConfigRefusal::Invalid)?);
    if unsafe {
        raw::git_config_add_backend(
            config.0.as_ptr(),
            backend.0.as_ptr(),
            raw::GIT_CONFIG_LEVEL_LOCAL,
            ptr::null(),
            0,
        )
    } < 0
    {
        return Err(ConfigRefusal::Invalid);
    }
    std::mem::forget(backend); // Config now owns the backend.
    let mut iterator = ptr::null_mut();
    if unsafe { raw::git_config_iterator_new(&mut iterator, config.0.as_ptr()) } < 0 {
        return Err(ConfigRefusal::Invalid);
    }
    let iterator = IteratorHandle(NonNull::new(iterator).ok_or(ConfigRefusal::Invalid)?);
    let mut result = Vec::new();
    loop {
        let mut entry = ptr::null_mut();
        let status = unsafe { raw::git_config_next(&mut entry, iterator.0.as_ptr()) };
        if status == raw::GIT_ITEROVER {
            break;
        }
        if status < 0 || entry.is_null() {
            return Err(ConfigRefusal::Invalid);
        }
        if result.len() == max_entries {
            return Err(ConfigRefusal::CapacityExceeded);
        }
        // Entry storage remains owned by the iterator/backend. Copy all values
        // before advancing or dropping it.
        let entry = unsafe { &*entry };
        if entry.name.is_null() {
            return Err(ConfigRefusal::Invalid);
        }
        let name = unsafe { CStr::from_ptr(entry.name) }
            .to_str()
            .map_err(|_| ConfigRefusal::Invalid)?
            .to_owned();
        let value = if entry.value.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(entry.value) }
                    .to_str()
                    .map_err(|_| ConfigRefusal::Invalid)?
                    .to_owned(),
            )
        };
        result.push(ConfigEntry { name, value });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_parser_preserves_multivars_escaping_and_unfollowed_includes() {
        let entries = parse(
            b"[core]\nfilemode = false\n[include]\npath = missing-file\n[custom \"Mixed.Case\"]\nvalue = \"first\\nline\"\nvalue = second\n",
            10,
        ).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].name, "core.filemode");
        assert_eq!(entries[1].name, "include.path");
        assert_eq!(entries[1].value.as_deref(), Some("missing-file"));
        assert_eq!(entries[2].name, "custom.Mixed.Case.value");
        assert_eq!(entries[2].value.as_deref(), Some("first\nline"));
        assert_eq!(entries[3].value.as_deref(), Some("second"));
    }

    #[test]
    fn memory_parser_refuses_invalid_or_over_capacity_without_partial_result() {
        assert_eq!(
            parse(b"[core]\nname=a\nname=b\n", 1),
            Err(ConfigRefusal::CapacityExceeded)
        );
        assert_eq!(parse(b"[core\n", 10), Err(ConfigRefusal::Invalid));
        assert_eq!(
            parse(b"[core]\nname=a\0tail", 10),
            Err(ConfigRefusal::Invalid)
        );
    }
}
