//! Synthetic public-graph completeness input for the V11 all-cfg extractor.
//!
//! This crate is deliberately independent of `symforge`. Its only purpose is to
//! prove that extraction sees inactive targets, negative predicates, impl edges,
//! associated items, auto-trait differences, and exported macros.

#![allow(dead_code)]

use std::rc::Rc;

pub struct AlwaysVisible;

#[cfg(target_arch = "aarch64")]
pub struct Aarch64Only;

#[cfg(target_arch = "x86_64")]
pub struct X8664Only;

#[cfg(target_os = "linux")]
pub struct LinuxOnly;

#[cfg(target_os = "macos")]
pub struct MacosOnly;

#[cfg(target_os = "windows")]
pub struct WindowsOnly;

#[cfg(target_env = "")]
pub struct EmptyEnvironmentOnly;

#[cfg(target_env = "gnu")]
pub struct GnuOnly;

#[cfg(target_env = "msvc")]
pub struct MsvcOnly;

#[cfg(target_env = "musl")]
pub struct MuslOnly;

#[cfg(target_family = "unix")]
pub struct UnixOnly;

#[cfg(target_family = "windows")]
pub struct WindowsFamilyOnly;

#[cfg(target_vendor = "apple")]
pub struct AppleOnly;

#[cfg(target_vendor = "pc")]
pub struct PcOnly;

#[cfg(target_vendor = "unknown")]
pub struct UnknownVendorOnly;

#[cfg(target_endian = "little")]
pub struct LittleEndianOnly;

#[cfg(target_pointer_width = "64")]
pub struct PointerWidth64Only;

#[cfg(target_has_atomic = "128")]
pub struct Atomic128;

#[cfg(not(target_has_atomic = "128"))]
pub struct NoAtomic128;

#[cfg(target_has_atomic = "ptr")]
pub struct AtomicPointerWidth;

#[cfg(feature = "embed")]
pub struct EmbedEnabled;

#[cfg(not(feature = "server"))]
pub struct NotServer;

#[cfg(all(feature = "embed", not(feature = "server")))]
pub struct PureEmbed;

#[cfg(feature = "cbm-spike")]
pub struct CbmSpikeEnabled;

pub trait CompletenessTrait {
    type Output;

    const MARKER: u8;

    fn apply(&self, input: u8) -> Self::Output;
}

pub struct CompletenessSubject;

impl CompletenessTrait for CompletenessSubject {
    type Output = u16;

    const MARKER: u8 = 11;

    fn apply(&self, input: u8) -> Self::Output {
        u16::from(input)
    }
}

pub struct CompletenessAutoTraits;

pub struct CompletenessNotSendSync(pub Rc<()>);

#[macro_export]
macro_rules! completeness_exported_macro {
    () => {
        11_u8
    };
}

