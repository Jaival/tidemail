//! Embed the app icon (`assets/tidemail.rc`) as resource 1.

fn main() {
    println!("cargo:rerun-if-changed=assets/tidemail.rc");
    println!("cargo:rerun-if-changed=assets/logo/tidemail.ico");
    embed_resource::compile("assets/tidemail.rc", embed_resource::NONE)
        .manifest_required()
        .expect("compile assets/tidemail.rc");
}
