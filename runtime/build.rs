fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc_path = protoc_bin_vendored::protoc_bin_path().unwrap();
    // Build script runs in single-threaded context; set_var is safe here.
    #[allow(unsafe_code)]
    unsafe {
        std::env::set_var("PROTOC", protoc_path);
    }
    // Use bytes::Bytes for all bytes fields in generated code so the replication
    // fan-out path can pass Bytes directly to proto without copying via to_vec().
    tonic_build::configure()
        .bytes(["."])
        .compile_protos(&["proto/wrela_rpc.proto"], &["proto"])?;
    Ok(())
}
