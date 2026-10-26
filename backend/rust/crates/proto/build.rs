//! Compiles /protos with a pure-Rust protobuf compiler (protox), so building
//! the backend never needs `protoc` installed.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../protos");
    let files = ["feast/common/v1/common.proto", "feast/notification/v1/notification.proto"];
    for f in &files {
        println!("cargo:rerun-if-changed={}", root.join(f).display());
    }
    let fds = protox::compile(files, [&root])?;
    tonic_build::configure().build_server(true).build_client(true).compile_fds(fds)?;
    Ok(())
}
