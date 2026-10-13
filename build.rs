#[path = "src/app_icon.rs"]
mod app_icon;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/app_icon.rs");
    println!("cargo:rerun-if-env-changed=WINDRES");
    println!("cargo:rerun-if-env-changed=RC");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must set OUT_DIR"));
    fs::write(output.join("app.ico"), windows_icon()).expect("Cannot write application icon");
    fs::write(output.join("app.rc"), resource_script())
        .expect("Cannot write application resource script");

    let resource = match env::var("CARGO_CFG_TARGET_ENV").as_deref() {
        Ok("gnu") | Ok("gnullvm") => {
            let target = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
                Ok("x86") => "pe-i386",
                Ok("x86_64") => "pe-x86-64",
                Ok("aarch64") => "pe-aarch64",
                architecture => {
                    panic!("Unsupported Windows resource architecture: {architecture:?}")
                }
            };
            let compiler = env::var_os("WINDRES").unwrap_or_else(|| "windres".into());
            let mut command = Command::new(compiler);
            command.current_dir(&output).args([
                "--target",
                target,
                "--input-format=rc",
                "--output-format=coff",
                "--input=app.rc",
                "--output=app-resource.o",
            ]);
            run_resource_compiler(
                &mut command,
                "Install MinGW binutils and add its bin directory to PATH, or set WINDRES to the full path of windres.exe.",
            );
            output.join("app-resource.o")
        }
        Ok("msvc") => {
            let compiler = env::var_os("RC").unwrap_or_else(|| "rc.exe".into());
            let mut command = Command::new(compiler);
            command
                .current_dir(&output)
                .args(["/nologo", "/fo", "app-resource.res", "app.rc"]);
            run_resource_compiler(
                &mut command,
                "Install the Windows SDK with Visual Studio C++ Build Tools and build from its Developer PowerShell (rc.exe must be on PATH), or set RC to the full path of rc.exe.",
            );
            output.join("app-resource.res")
        }
        environment => panic!("Unsupported Windows resource toolchain: {environment:?}"),
    };
    link_resource(&resource);
}

fn run_resource_compiler(command: &mut Command, requirement: &str) {
    let result = command.output().unwrap_or_else(|error| {
        panic!("Cannot run Windows resource compiler: {error}. {requirement}")
    });
    assert!(
        result.status.success(),
        "Windows resource compiler failed. {requirement}\n{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr),
    );
}

fn link_resource(resource: &Path) {
    println!(
        "cargo:rustc-link-arg-bin=codex-context-manager={}",
        resource.display()
    );
}

fn resource_script() -> String {
    let version = env::var("CARGO_PKG_VERSION").expect("Cargo must set package version");
    let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
    let minor = env::var("CARGO_PKG_VERSION_MINOR").unwrap();
    let patch = env::var("CARGO_PKG_VERSION_PATCH").unwrap();
    format!(
        r#"1 ICON "app.ico"

1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "CompanyName", "Codex Context Manager\0"
            VALUE "FileDescription", "Codex Context Manager\0"
            VALUE "FileVersion", "{version}\0"
            VALUE "InternalName", "codex-context-manager\0"
            VALUE "OriginalFilename", "codex-context-manager.exe\0"
            VALUE "ProductName", "Codex Context Manager\0"
            VALUE "ProductVersion", "{version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x409, 1200
    END
END
"#
    )
}

fn windows_icon() -> Vec<u8> {
    let sizes = [16_u32, 24, 32, 48, 64, 128, 256];
    let images: Vec<_> = sizes.iter().map(|&size| icon_bitmap(size)).collect();
    let mut icon = Vec::new();
    icon.extend_from_slice(&[0, 0, 1, 0]);
    icon.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = (6 + sizes.len() * 16) as u32;

    for (&size, bitmap) in sizes.iter().zip(&images) {
        // A zero width/height byte represents the 256-pixel ICO entry.
        icon.extend_from_slice(&[size as u8, size as u8, 0, 0]);
        icon.extend_from_slice(&1_u16.to_le_bytes());
        icon.extend_from_slice(&32_u16.to_le_bytes());
        icon.extend_from_slice(&(bitmap.len() as u32).to_le_bytes());
        icon.extend_from_slice(&offset.to_le_bytes());
        offset += bitmap.len() as u32;
    }
    for bitmap in images {
        icon.extend_from_slice(&bitmap);
    }
    icon
}

fn icon_bitmap(size: u32) -> Vec<u8> {
    let pixels = app_icon::rgba(size);
    let mask_stride = size.div_ceil(32) * 4;
    let mask_bytes = mask_stride * size;
    let mut bitmap = Vec::new();
    bitmap.extend_from_slice(&40_u32.to_le_bytes());
    bitmap.extend_from_slice(&size.to_le_bytes());
    bitmap.extend_from_slice(&(size * 2).to_le_bytes());
    bitmap.extend_from_slice(&1_u16.to_le_bytes());
    bitmap.extend_from_slice(&32_u16.to_le_bytes());
    bitmap.extend_from_slice(&0_u32.to_le_bytes());
    bitmap.extend_from_slice(&(size * size * 4 + mask_bytes).to_le_bytes());
    bitmap.extend_from_slice(&[0; 16]);

    for y in (0..size).rev() {
        for x in 0..size {
            let offset = ((y * size + x) * 4) as usize;
            bitmap.extend_from_slice(&[
                pixels[offset + 2],
                pixels[offset + 1],
                pixels[offset],
                pixels[offset + 3],
            ]);
        }
    }
    for y in (0..size).rev() {
        let mut mask = vec![0; mask_stride as usize];
        for x in 0..size {
            if pixels[((y * size + x) * 4 + 3) as usize] == 0 {
                mask[(x / 8) as usize] |= 0x80 >> (x % 8);
            }
        }
        bitmap.extend_from_slice(&mask);
    }
    bitmap
}
