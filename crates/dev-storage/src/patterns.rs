//! File-name and path heuristics used after structural marker checks.

use std::{ffi::OsStr, path::Path};

pub(crate) fn lower(name: &OsStr) -> String {
    name.to_string_lossy().to_lowercase()
}

pub(crate) fn extension(name: &OsStr) -> String {
    Path::new(name)
        .extension()
        .map_or_else(String::new, |extension| {
            format!(".{}", extension.to_string_lossy().to_lowercase())
        })
}

pub(crate) fn is_vm_image(name: &OsStr) -> bool {
    matches!(
        extension(name).as_str(),
        ".vhd" | ".vhdx" | ".vmdk" | ".qcow2" | ".vdi"
    )
}

pub(crate) fn is_rust_artifact(name: &OsStr) -> bool {
    matches!(extension(name).as_str(), ".rlib" | ".rmeta")
}

pub(crate) fn is_rust_artifact_directory(name: &OsStr) -> bool {
    matches!(
        lower(name).as_str(),
        "debug" | "release" | "deps" | "incremental" | ".fingerprint"
    )
}

pub(crate) fn is_visual_studio_directory(name: &OsStr) -> bool {
    matches!(
        lower(name).as_str(),
        ".vs" | "bin" | "obj" | "debug" | "release" | "x64" | "win32"
    )
}

pub(crate) fn has_visual_studio_project_extension(name: &OsStr) -> bool {
    matches!(
        extension(name).as_str(),
        ".sln" | ".csproj" | ".vcxproj" | ".fsproj"
    )
}

pub(crate) fn is_yarn_project_cache(path: &Path) -> bool {
    path.file_name().is_some_and(|name| lower(name) == "cache")
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| lower(name) == ".yarn")
}

/// Recognise Docker Desktop and WSL virtual disks by path as well as by the
/// explicitly registered known locations. No file is opened by this check.
pub(crate) fn is_docker_or_wsl_disk(path: &Path) -> bool {
    if !path
        .extension()
        .is_some_and(|extension| lower(extension) == "vhdx")
    {
        return false;
    }
    let segments: Vec<String> = path
        .components()
        .map(|component| lower(component.as_os_str()))
        .collect();
    let has = |wanted: &str| segments.iter().any(|segment| segment == wanted);
    let docker_wsl = has("docker") && has("wsl");
    let wsl_package = has("packages") && has("localstate");
    let named_docker_disk = path.file_name().is_some_and(|name| {
        matches!(
            lower(name).as_str(),
            "docker_data.vhdx" | "dockerdesktop.vhdx"
        )
    });
    docker_wsl || wsl_package || (named_docker_disk && has("dockerdesktop"))
}
