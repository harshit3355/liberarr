use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../../web/dist");
    if !Path::new("../../web/dist/index.html").exists() {
        panic!("run `npm --prefix web ci && npm --prefix web run build` first");
    }
}
