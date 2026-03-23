fn main() {
    println!("cargo:rerun-if-changed=proto/driver.proto");

    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile_protos(&["proto/driver.proto"], &["proto"])
        .expect("failed to compile driver proto");
}
