fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("proto");

    println!(
        "cargo:rerun-if-changed={}",
        proto_root.join("weyvelength.proto").display()
    );

    let fds = protox::compile(["weyvelength.proto"], [&proto_root])?;
    tonic_prost_build::configure()
        .build_server(true)
        .build_client(false)
        .compile_fds(fds)?;
    Ok(())
}
