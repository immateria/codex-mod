pub(crate) use code_core::history::*;

#[cfg(feature = "code-fork")]
pub(crate) mod compat;

pub(crate) mod state {
    pub(crate) use code_core::history::*;
}
