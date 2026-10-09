fn main() {
    let manifest_path = std::path::Path::new("nikbvirus.manifest")
        .canonicalize()
        .unwrap();
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest_path.display()
    );
}
