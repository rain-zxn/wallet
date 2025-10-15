use std::env::var;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::fs;

const REPO_URL_HTTPS: &str = "https://github.com/theparadigmshifters/wallet_prover";
const REPO_URL_SSH: &str = "git@github.com:theparadigmshifters/wallet_prover.git";
const BRANCH: &str = "main";
const REPO_DIR: &str = "wallet_prover";

fn main() {
    let out_dir = PathBuf::from(var("OUT_DIR").unwrap());
    let repo_path = out_dir.join(REPO_DIR);
    
    check_dependencies();
    setup_repository(&repo_path);

    let go_pkg_path = find_main_go(&repo_path)
        .expect("Could not find main.go in repository");

    download_go_dependencies(&go_pkg_path);
    let lib_path = build_go_library(&go_pkg_path, &out_dir);
    copy_to_output_dir(&lib_path);
    setup_linking(&out_dir);

    println!("cargo:rerun-if-changed={}", go_pkg_path.join("main.go").display());
    println!("cargo:rerun-if-changed={}", go_pkg_path.join("go.mod").display());
    println!("cargo:rerun-if-changed={}", go_pkg_path.join("wallet_prover_ffi.go").display());
}

fn check_dependencies() {
    if Command::new("git").arg("--version").output().is_err() {
        panic!("git is not installed or not in PATH");
    }

    if Command::new("go").arg("version").output().is_err() {
        panic!("go is not installed or not in PATH");
    }
}

fn setup_repository(repo_path: &Path) {
    if !repo_path.exists() {
        clone_repository(repo_path);
    } else {
        eprintln!("Repository already exists at {}", repo_path.display());
    }
}

fn clone_repository(repo_path: &Path) {
    let use_ssh = var("GIT_USE_SSH").is_ok() || check_ssh_available();
    let repo_url = if use_ssh { REPO_URL_SSH } else { REPO_URL_HTTPS };
    
    eprintln!("Cloning repository from {} ({})", repo_url, if use_ssh { "SSH" } else { "HTTPS" });
    
    let status = Command::new("git")
        .args([
            "clone",
            "--branch", BRANCH,
            "--depth", "1",
            repo_url,
            repo_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute git clone");

    if !status.success() {
        if use_ssh {
            eprintln!("SSH clone failed, trying HTTPS...");
            let status = Command::new("git")
                .args([
                    "clone",
                    "--branch", BRANCH,
                    "--depth", "1",
                    REPO_URL_HTTPS,
                    repo_path.to_str().unwrap(),
                ])
                .status()
                .expect("Failed to execute git clone");
            
            if !status.success() {
                panic!("Git clone failed with both SSH and HTTPS");
            }
        } else {
            panic!("Git clone failed");
        }
    }
}

fn check_ssh_available() -> bool {
    Command::new("ssh")
        .args(["-T", "git@github.com"])
        .output()
        .map(|output| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            stderr.contains("successfully authenticated")
        })
        .unwrap_or(false)
}

fn find_main_go(repo_path: &Path) -> Option<PathBuf> {
    let common_paths = [".", "cmd/prover", "prover", "cmd"];
    
    for path in &common_paths {
        let candidate = repo_path.join(path).join("main.go");
        if candidate.exists() {
            eprintln!("Found main.go at: {}", candidate.display());
            return Some(repo_path.join(path));
        }
    }

    eprintln!("Searching for main.go recursively...");
    find_recursive(repo_path, 0, 3)
}

fn find_recursive(dir: &Path, depth: usize, max: usize) -> Option<PathBuf> {
    if depth > max {
        return None;
    }

    if dir.join("main.go").exists() {
        eprintln!("Found main.go at: {}", dir.display());
        return Some(dir.to_path_buf());
    }

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(file_type) = entry.file_type() {
                if file_type.is_dir() {
                    let name = entry.file_name();
                    if name == ".git" || name == "vendor" || name == "node_modules" || name == "target" {
                        continue;
                    }
                    
                    if let Some(found) = find_recursive(&entry.path(), depth + 1, max) {
                        return Some(found);
                    }
                }
            }
        }
    }
    None
}

fn download_go_dependencies(go_src: &Path) {
    let go_mod_dir = if go_src.join("go.mod").exists() {
        go_src
    } else if let Some(parent) = go_src.parent() {
        if parent.join("go.mod").exists() { 
            parent 
        } else { 
            go_src 
        }
    } else {
        go_src
    };

    eprintln!("Downloading Go dependencies...");
    
    let mut cmd = Command::new("go");
    cmd.args(["mod", "download"])
        .current_dir(go_mod_dir);
    
    if let Ok(proxy) = var("GOPROXY") {
        cmd.env("GOPROXY", proxy);
    } else {
        cmd.env("GOPROXY", "https://proxy.golang.org,direct");
    }

    if let Ok(ssh_key) = var("GIT_SSH_COMMAND") {
        cmd.env("GIT_SSH_COMMAND", ssh_key);
    }

    let status = cmd.status().expect("Failed to execute go mod download");
    if !status.success() {
        panic!("go mod download failed");
    }

    let mut tidy_cmd = Command::new("go");
    tidy_cmd.args(["mod", "tidy"])
        .current_dir(go_mod_dir);

    if let Ok(ssh_key) = var("GIT_SSH_COMMAND") {
        tidy_cmd.env("GIT_SSH_COMMAND", ssh_key);
    }

    let status = tidy_cmd.status().expect("Failed to execute go mod tidy");
    if !status.success() {
        panic!("go mod tidy failed");
    }
}

fn build_go_library(go_src: &Path, out_dir: &Path) -> PathBuf {
    let lib_name = "libwallet_prover.a";
    let header_name = "libwallet_prover.h";
    
    let lib_path = out_dir.join(lib_name);
    let header_path = out_dir.join(header_name);

    let ffi_file = go_src.join("wallet_prover_ffi.go");
    if !ffi_file.exists() {
        panic!("wallet_prover_ffi.go not found at {}", ffi_file.display());
    }

    eprintln!("Building Go static library from {}...", ffi_file.display());

    let mut cmd = Command::new("go");
    cmd.args([
            "build",
            "-buildmode=c-archive", 
            "-ldflags", "-w -s -extldflags=-static",
            "-o",
            lib_path.to_str().unwrap(),
            "wallet_prover_ffi.go",
        ])
        .current_dir(go_src);

    // 启用CGO并设置静态链接
    cmd.env("CGO_ENABLED", "1");
    
    #[cfg(target_os = "linux")]
    {
        cmd.env("CGO_CFLAGS", "-static")
           .env("CGO_LDFLAGS", "-static");
    }

    #[cfg(target_os = "macos")]
    {
        cmd.env("CGO_CFLAGS", "-mmacosx-version-min=11.0")
           .env("CGO_LDFLAGS", "-mmacosx-version-min=11.0");
    }

    #[cfg(target_os = "windows")]
    {
        cmd.env("CGO_LDFLAGS", "-static-libgcc -static-libstdc++");
    }

    let status = cmd.status().expect("Failed to execute go build");
    if !status.success() {
        panic!("Go build failed");
    }

    eprintln!("Go static library built at {}", lib_path.display());
    
    if header_path.exists() {
        eprintln!("Header file generated at {}", header_path.display());
    }
    
    lib_path
}

fn copy_to_output_dir(lib_path: &Path) {
    let profile = var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    let manifest_dir = var("CARGO_MANIFEST_DIR").unwrap();
    
    let target_dir = PathBuf::from(&manifest_dir)
        .join("target")
        .join(&profile);
    
    if let Some(lib_name) = lib_path.file_name() {
        let dest = target_dir.join(lib_name);
        
        if let Ok(_) = fs::create_dir_all(&target_dir) {
            match fs::copy(lib_path, &dest) {
                Ok(_) => eprintln!("Copied static library to {}", dest.display()),
                Err(e) => eprintln!("Warning: failed to copy library: {}", e),
            }
        }
    }

    let header_path = lib_path.with_extension("h");
    if header_path.exists() {
        if let Some(header_name) = header_path.file_name() {
            let dest = target_dir.join(header_name);
            match fs::copy(&header_path, &dest) {
                Ok(_) => eprintln!("Copied header file to {}", dest.display()),
                Err(e) => eprintln!("Warning: failed to copy header: {}", e),
            }
        }
    }

    eprintln!("Static linking configured - no runtime dependencies required");
}

fn setup_linking(out_dir: &Path) {
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    
    println!("cargo:rustc-link-lib=static=wallet_prover");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
        println!("cargo:rustc-link-lib=framework=Security");
        println!("cargo:rustc-link-lib=dylib=resolv");
        println!("cargo:rustc-link-lib=dylib=c");
    }

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-lib=dylib=m");
        println!("cargo:rustc-link-lib=dylib=resolv");
        println!("cargo:rustc-link-lib=dylib=c");
        // println!("cargo:rustc-link-lib=static=c");
    }

    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=dylib=ws2_32");
        println!("cargo:rustc-link-lib=dylib=userenv");
        println!("cargo:rustc-link-lib=dylib=advapi32");
    }

    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");

    #[cfg(target_os = "linux")]
    {
        println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
    }
}
