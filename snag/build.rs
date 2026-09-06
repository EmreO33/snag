//! Attaches the Windows resources: the executable's own icon and its version
//! block, so Explorer and the taskbar show the Snag mark rather than a blank.

fn main() {
    println!("cargo:rerun-if-changed=../assets/icon.ico");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icon.ico");
        res.set("ProductName", "Snag");
        res.set("FileDescription", "Snag");
        res.set("LegalCopyright", "");
        if let Err(e) = res.compile() {
            // A missing resource compiler should not stop anyone building the
            // app; it only costs the icon.
            println!("cargo:warning=could not attach the windows icon: {e}");
        }
    }
}
