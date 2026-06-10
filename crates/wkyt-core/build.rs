fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=proto/delta.proto");
    // protox compiles the .proto in pure Rust (no system protoc needed),
    // producing a FileDescriptorSet that prost-build turns into Rust types.
    let fds = protox::compile(["proto/delta.proto"], ["proto"])?;
    prost_build::Config::new().compile_fds(fds)?;
    Ok(())
}
