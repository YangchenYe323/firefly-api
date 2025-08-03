// build.rs
fn main() {
    println!("cargo::rustc-check-cfg=cfg(ci_build)");
    if std::env::var("CI").is_ok() {
        println!("cargo:rustc-cfg=ci_build");
    }
}