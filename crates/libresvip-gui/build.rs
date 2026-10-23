//! Embeds the Windows icon, manifest and version resource into the GUI binary,
//! mirroring what the PyInstaller build of the original ships.

fn main() {
    #[cfg(windows)]
    {
        // `embed-resource` locates `rc.exe` through the Windows SDK.
        println!("cargo:rerun-if-changed=assets/app.rc");
        println!("cargo:rerun-if-changed=assets/app.manifest");
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let _ = embed_resource::compile("assets/app.rc", embed_resource::NONE);
    }
}
