fn main() {
    // App icon for agent-pals.exe and hook-relay.exe (needs rc.exe from the Windows SDK).
    println!("cargo:rerun-if-changed=assets/agent-pals.rc");
    println!("cargo:rerun-if-changed=assets/agent-pals.ico");
    embed_resource::compile("assets/agent-pals.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}
