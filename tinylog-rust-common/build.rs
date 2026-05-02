fn main() {
    let proto_root = "../tinylog-core/src/main/proto";
    let proto_file = "../tinylog-core/src/main/proto/tinylog/prototype.proto";

    println!("cargo:rerun-if-changed={proto_file}");

    let protoc_path =
        protoc_bin_vendored::protoc_bin_path().expect("failed to locate vendored protoc");
    std::env::set_var("PROTOC", protoc_path);

    let mut config = prost_build::Config::new();
    config.protoc_arg("--experimental_allow_proto3_optional");
    config
        .compile_protos(&[proto_file], &[proto_root])
        .expect("failed to compile TinyLog protobuf contract");
}
