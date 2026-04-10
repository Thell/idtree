// build.rs

fn main() {
    #[cfg(feature = "cpp")]
    {
        cxx_build::bridge("src/bridge.rs")
            .file("cpp/dndtree_wrapper.cpp")
            // No need to compile dndtree.h if it's all inline/included
            .include("cpp")
            .flag_if_supported("/std:c++20")
            .flag_if_supported("-std=c++20")
            // This suppresses the MSVC warnings about fopen
            .define("_CRT_SECURE_NO_WARNINGS", None)
            .compile("dndtree-bridge");

        println!("cargo:rerun-if-changed=src/bridge.rs");
        println!("cargo:rerun-if-changed=cpp/dndtree_wrapper.cpp");
        println!("cargo:rerun-if-changed=cpp/dndtree_wrapper.h");
        println!("cargo:rerun-if-changed=cpp/dndtree.h");
    }
}
