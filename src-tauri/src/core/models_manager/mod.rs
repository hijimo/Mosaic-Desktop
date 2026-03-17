pub mod cache;
pub mod manager;
pub mod model_info;

/// Convert the crate version to a `MAJOR.MINOR.PATCH` string.
pub fn client_version_to_whole() -> String {
    format!(
        "{}.{}.{}",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH")
    )
}
