use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=kras-quick.ico");
    println!("cargo:rerun-if-changed=kras-quick.rc");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") { return; }
    let sdk = PathBuf::from(std::env::var_os("ProgramFiles(x86)").expect("Windows SDK location is unavailable"))
        .join("Windows Kits/10/bin");
    let rc = std::env::var_os("RC").map(PathBuf::from).or_else(|| {
        std::fs::read_dir(sdk).ok()?.filter_map(Result::ok)
            .map(|entry| entry.path().join("x64/rc.exe"))
            .filter(|path| path.is_file()).max()
    }).expect("Windows SDK rc.exe is required to embed the application icon");
    let resource = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is missing")).join("kras-quick.res");
    let status = std::process::Command::new(rc).arg("/nologo").arg("/fo").arg(&resource)
        .arg("kras-quick.rc").status().expect("Failed to run the Windows resource compiler");
    assert!(status.success(), "Application icon resource compilation failed");
    println!("cargo:rustc-link-arg-bin=kras-quick={}", resource.display());
}
