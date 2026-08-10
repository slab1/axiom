//! Passes and pass managers.

// pub mod r#async; // disabled: melior-macro 0.8.1 missing async_passes! macro
// pub mod conversion; // disabled: mlir-sys 0.2.2 (MLIR18) missing ConvertLinalgToLLVM symbols
pub mod external;
// pub mod gpu; // disabled: melior-macro 0.8.1 missing gpu_passes! macro
// pub mod linalg; // disabled: melior-macro 0.8.1 missing linalg_passes! macro
mod manager;
mod operation_manager;
// pub mod sparse_tensor; // disabled: melior-macro 0.8.1 missing sparse_tensor_passes! macro
// pub mod transform; // disabled: melior-macro 0.8.1 missing transform_passes! macro

pub use self::{
    external::{create_external, ExternalPass, RunExternalPass},
    manager::PassManager,
    operation_manager::OperationPassManager,
};
use mlir_sys::MlirPass;

/// A pass.
pub struct Pass {
    raw: MlirPass,
}

impl Pass {
    /// Creates a pass from a raw function.
    ///
    /// # Safety
    ///
    /// A raw function must be valid.
    pub unsafe fn from_raw_fn(create_raw: unsafe extern "C" fn() -> MlirPass) -> Self {
        Self {
            raw: unsafe { create_raw() },
        }
    }

    /// Creates a pass from a raw object.
    ///
    /// # Safety
    ///
    /// A raw object must be valid.
    pub const unsafe fn from_raw(raw: MlirPass) -> Self {
        Self { raw }
    }

    /// Converts a pass into a raw object.
    pub const fn to_raw(&self) -> MlirPass {
        self.raw
    }

    #[doc(hidden)]
    pub unsafe fn __private_from_raw_fn(create_raw: unsafe extern "C" fn() -> MlirPass) -> Self {
        Self::from_raw_fn(create_raw)
    }
}
