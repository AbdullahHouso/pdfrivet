//! Developer tasks for the Rivet workspace. Run with `cargo xtask <task>`.
//!
//! Tasks:
//! - `fetch-pdfium [--platform <name>]`: download the prebuilt PDFium library
//!   from bblanchon/pdfium-binaries into `vendor/pdfium/<platform>/`.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};

/// The PDFium release we build against. Bump this (and test!) to update PDFium.
const PDFIUM_RELEASE: &str = "chromium/8086";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("fetch-pdfium") => {
            let platform = match args.iter().position(|a| a == "--platform") {
                Some(i) => args.get(i + 1).context("--platform needs a value")?.clone(),
                None => host_platform()?.to_owned(),
            };
            fetch_pdfium(&platform)
        }
        _ => {
            eprintln!("usage: cargo xtask fetch-pdfium [--platform win-x64|linux-x64|mac-univ]");
            std::process::exit(2);
        }
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// Maps the current OS/CPU to the platform names used by pdfium-binaries.
fn host_platform() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "win-x64",
        ("windows", "aarch64") => "win-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        // One universal library covers both Intel and Apple Silicon Macs.
        ("macos", _) => "mac-univ",
        (os, arch) => bail!("no prebuilt PDFium for {os}/{arch}"),
    })
}

/// File name of the PDFium shared library on the given platform.
fn library_name(platform: &str) -> &'static str {
    if platform.starts_with("win") {
        "pdfium.dll"
    } else if platform.starts_with("mac") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    }
}

fn fetch_pdfium(platform: &str) -> Result<()> {
    let dest = workspace_root().join("vendor/pdfium").join(platform);
    let stamp = dest.join("RELEASE");
    if fs::read_to_string(&stamp).is_ok_and(|s| s.trim() == PDFIUM_RELEASE) {
        println!(
            "PDFium {PDFIUM_RELEASE} for {platform} is already in {}",
            dest.display()
        );
        return Ok(());
    }

    let url = format!(
        "https://github.com/bblanchon/pdfium-binaries/releases/download/{}/pdfium-{platform}.tgz",
        PDFIUM_RELEASE.replace('/', "%2F")
    );
    println!("Downloading {url}");
    let mut archive = Vec::new();
    ureq::get(&url)
        .call()
        .with_context(|| format!("download failed: {url}"))?
        .into_body()
        .into_reader()
        .read_to_end(&mut archive)?;

    // Start clean so files from an older release never linger.
    if dest.exists() {
        fs::remove_dir_all(&dest)?;
    }
    fs::create_dir_all(&dest)?;

    // Keep only the shared library and license files; skip headers and build metadata.
    let lib = library_name(platform);
    let mut found_lib = false;
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive.as_slice()));
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let keep = path.file_name().is_some_and(|n| n == lib)
            || path.starts_with("LICENSE")
            || path.starts_with("licenses");
        if !keep || entry.header().entry_type().is_dir() {
            continue;
        }
        let out = if path.file_name().is_some_and(|n| n == lib) {
            found_lib = true;
            dest.join(lib)
        } else {
            dest.join(&path)
        };
        fs::create_dir_all(out.parent().unwrap())?;
        entry.unpack(&out)?;
    }
    if !found_lib {
        bail!("{lib} was not found in the PDFium archive");
    }
    fs::write(&stamp, PDFIUM_RELEASE)?;
    println!(
        "PDFium {PDFIUM_RELEASE} for {platform} is ready in {}",
        dest.display()
    );
    Ok(())
}
