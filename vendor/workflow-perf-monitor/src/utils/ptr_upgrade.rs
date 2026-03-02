use std::num::NonZeroIsize;

/// Triage a return value of windows handle to `Some(handle)` or `None`
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub trait HandleUpgrade: Sized {
    fn upgrade(self) -> Option<NonZeroIsize>;
}

impl HandleUpgrade for isize {
    #[inline]
    fn upgrade(self) -> Option<NonZeroIsize> {
        NonZeroIsize::new(self)
    }
}
