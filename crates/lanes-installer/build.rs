fn main() {
    if std::env::var_os("CARGO_CFG_WINDOWS").is_some() {
        slint_build::compile("ui/installer.slint").expect("compile installer UI");
    }
    if std::env::var_os("CARGO_FEATURE_BUNDLED").is_some() {
        let source = std::env::var_os("LANES_PAYLOAD_DIR")
            .expect("LANES_PAYLOAD_DIR must point to the built Windows binaries");
        let destination = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
        for binary in ["lanes.exe", "lanes-desktop.exe"] {
            let path = std::path::PathBuf::from(&source).join(binary);
            println!("cargo:rerun-if-changed={}", path.display());
            std::fs::copy(&path, destination.join(binary))
                .unwrap_or_else(|error| panic!("cannot bundle {}: {error}", path.display()));
        }
    }
}
