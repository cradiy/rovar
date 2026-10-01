fn main() {
    println!("cargo:rerun-if-changed=../../packaging/windows/rovar.rc");
    println!("cargo:rerun-if-changed=../../assets/rovar-icon.ico");
    #[cfg(windows)]
    embed_resource::compile_for(
        "../../packaging/windows/rovar.rc",
        ["rovar"],
        embed_resource::NONE,
    )
    .manifest_required()
    .expect("Could not embed the Windows application icon");
}
