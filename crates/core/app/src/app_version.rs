/// Representation of the Shieldd application version. Notably, this is distinct
/// from the crate version(s). This number should only ever be incremented.
pub const APP_VERSION: u64 = 14;

cfg_if::cfg_if! {
    if #[cfg(feature="component")] {
        mod component;
        pub use component::check_app_version;
        pub(crate) use component::initialize_app_version;
    }
}
