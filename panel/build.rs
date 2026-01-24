fn main() -> Result<(), Box<dyn std::error::Error>> {
    // tonic_build::configure()
    //     .build_server(false)
    //     .build_client(true)
    //     .compile(&["../proto/node.proto"], &["../proto"])?;
    tonic_build::compile_protos("../proto/node.proto")?;
    Ok(())
}
