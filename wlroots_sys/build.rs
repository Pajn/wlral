extern crate bindgen;
#[cfg(feature = "static")]
extern crate meson;
extern crate pkg_config;
extern crate wayland_scanner;

use std::path::PathBuf;
use std::process::Command;
use std::{env, fs, io};

fn main() {
  let protocol_header_path =
    generate_protocol_headers().expect("Could not generate header files for wayland protocols");
  let target_dir = env::var("OUT_DIR").expect("$OUT_DIR not set!");
  let mut builder = bindgen::builder()
    .derive_debug(true)
    .derive_default(true)
    .generate_comments(true)
    .header("src/wlroots.h")
    .whitelist_type(r"^wlr_.*$")
    .whitelist_function(r"^_?pixman_.*$")
    .whitelist_function(r"^_?wlr_.*$")
    .ctypes_prefix("libc")
    .clang_arg("-Iwlroots/include")
    .clang_arg("-Iwlroots/include/wlr")
    // NOTE Necessary because they use the out directory to put
    // pragma information on what features are available in a header file
    // titled "config.h"
    .clang_arg(format!("-I{}{}", target_dir, "/include/"))
    .clang_arg(format!("-I{}", protocol_header_path.to_str().unwrap()))
    .clang_arg("-Iwlroots/include/xcursor")
    .clang_arg("-I/usr/include/pixman-1")
    // Work around bug https://github.com/rust-lang-nursery/rust-bindgen/issues/687
    .blacklist_type("FP_NAN")
    .blacklist_type("FP_INFINITE")
    .blacklist_type("FP_ZERO")
    .blacklist_type("FP_SUBNORMAL")
    .blacklist_type("FP_NORMAL")
    .blacklist_type(r"^xkb_.*$")
    .blacklist_type(r"^XKB_.*$")
    // Work around duplicate wayland types from wlroots and wayland_sys
    .blacklist_type(r"^wl_(client|display|list|listener|resource|signal)$");
  if cfg!(feature = "unstable") {
    builder = builder
      .clang_arg("-DWLR_USE_UNSTABLE")
      .raw_line("use wayland_server::sys::common::*;")
      .raw_line("use wayland_server::sys::server::*;")
      .raw_line("use xkbcommon::xkb::ffi::*;");
  }
  if !cfg!(feature = "static") {
    // config.h won't exist, so make a dummy file.
    // We don't need it because of the following -D defines.
    fs::create_dir_all(format!("{}{}", target_dir, "/include/wlr/"))
      .expect("Could not create <out>/include/wlr");
    fs::File::create(format!("{}{}", target_dir, "/include/wlr/config.h"))
      .expect("Could not create dummy config.h file");
    // meson automatically sets up variables, but if we are linking
    // dynamically bindgen will no longer have them.
    builder = builder.clang_args(
      [
        format!("-DWLR_HAS_LIBCAP={}", cfg!(feature = "libcap") as u8),
        format!("-DWLR_HAS_SYSTEMD={}", cfg!(feature = "systemd") as u8),
        format!(
          "-DWLR_HAS_EGLMESAEXT_H={}",
          cfg!(feature = "eglmesaext_h") as u8
        ),
        format!("-DWLR_HAS_ELOGIND={}", cfg!(feature = "elogind") as u8),
        format!(
          "-DWLR_HAS_X11_BACKEND={}",
          cfg!(feature = "x11_backend") as u8
        ),
        format!("-DWLR_HAS_XWAYLAND={}", cfg!(feature = "xwayland") as u8),
        format!(
          "-DWLR_HAS_XCB_ERRORS={}",
          cfg!(feature = "xcb_errors") as u8
        ),
        format!("-DWLR_HAS_XCB_ICCCM={}", cfg!(feature = "xcb_icccm") as u8),
      ]
      .iter(),
    )
  }
  let generated = builder.generate().unwrap();

  println!("cargo:rustc-link-lib=dylib=X11");
  println!("cargo:rustc-link-lib=dylib=X11-xcb");
  println!("cargo:rustc-link-lib=dylib=xkbcommon");
  println!("cargo:rustc-link-lib=dylib=xcb");
  println!("cargo:rustc-link-lib=dylib=xcb-composite");
  println!("cargo:rustc-link-lib=dylib=xcb-xfixes");
  println!("cargo:rustc-link-lib=dylib=xcb-image");
  println!("cargo:rustc-link-lib=dylib=xcb-render");
  println!("cargo:rustc-link-lib=dylib=xcb-shm");
  println!("cargo:rustc-link-lib=dylib=xcb-icccm");
  println!("cargo:rustc-link-lib=dylib=xcb-xkb");
  println!("cargo:rustc-link-lib=dylib=xcb-xinput");
  println!("cargo:rustc-link-lib=dylib=wayland-egl");
  println!("cargo:rustc-link-lib=dylib=wayland-client");
  println!("cargo:rustc-link-lib=dylib=wayland-server");
  println!("cargo:rustc-link-lib=dylib=EGL");
  println!("cargo:rustc-link-lib=dylib=GL");
  println!("cargo:rustc-link-lib=dylib=gbm");
  println!("cargo:rustc-link-lib=dylib=drm");
  println!("cargo:rustc-link-lib=dylib=input");
  println!("cargo:rustc-link-lib=dylib=udev");
  println!("cargo:rustc-link-lib=dylib=dbus-1");
  println!("cargo:rustc-link-lib=dylib=pixman-1");

  link_optional_libs();

  if !cfg!(feature = "static") {
    println!("cargo:rustc-link-lib=dylib=wlroots");
    println!("cargo:rustc-link-search=native=/usr/local/lib");
  }

  // generate the bindings
  println!("cargo:rerun-if-changed=src/wlroots.h");
  generated.write_to_file("src/gen.rs").unwrap();

  generate_protocols();
}

/// Gets the unstable and stable protocols in /usr/share-wayland-protocols and
/// in wlroots/protocol.
///
/// The path to the folder with the generated headers is returned. It will
/// have two directories, `stable`, and `unstable`.
fn generate_protocol_headers() -> io::Result<PathBuf> {
  let output_dir_str = env::var("OUT_DIR").unwrap();
  let out_path: PathBuf = format!("{}/wayland-protocols", output_dir_str).into();
  fs::create_dir(&out_path).ok();
  let protocols_prefix = pkg_config::get_variable("wayland-protocols", "prefix").unwrap();
  let protocols = fs::read_dir(format!(
    "{}/share/wayland-protocols/stable",
    protocols_prefix
  ))?
  .chain(fs::read_dir(format!(
    "{}/share/wayland-protocols/unstable",
    protocols_prefix
  ))?);
  for entry in protocols {
    let entry = entry?;
    for entry in fs::read_dir(entry.path())? {
      let entry = entry?;
      let path = entry.path();
      let mut filename = entry.file_name().into_string().unwrap();
      if filename.ends_with(".xml") {
        let new_length = filename.len() - 4;
        filename.truncate(new_length);
      }
      filename.push_str("-protocol");
      Command::new("wayland-scanner")
        .arg("server-header")
        .arg(path.clone())
        .arg(format!("{}/{}.h", out_path.to_str().unwrap(), filename))
        .status()
        .unwrap();
    }
  }
  for entry in fs::read_dir("./wlroots/protocol")? {
    let entry = entry?;
    let path = entry.path();
    let mut filename = entry.file_name().into_string().unwrap();
    if filename.ends_with(".xml") {
      let new_length = filename.len() - 4;
      filename.truncate(new_length);
    } else {
      continue;
    }
    filename.push_str("-protocol");
    Command::new("wayland-scanner")
      .arg("server-header")
      .arg(path.clone())
      .arg(format!("{}/{}.h", out_path.to_str().unwrap(), filename))
      .status()
      .unwrap();
  }

  Ok(out_path)
}

fn generate_protocols() {
  // let output_dir = Path::new(&"src");

  // let protocols = &[
  //   (
  //     "./wlroots/protocol/wlr-layer-shell-unstable-v1.xml",
  //     "layer_shell",
  //   ),
  // ];

  // for protocol in protocols {
  //   wayland_scanner::generate_code(
  //     protocol.0,
  //     output_dir.join(format!("{}_server_api.rs", protocol.1)),
  //     wayland_scanner::Side::Server,
  //   );
  //   wayland_scanner::generate_code(
  //     protocol.0,
  //     output_dir.join(format!("{}_client_api.rs", protocol.1)),
  //     wayland_scanner::Side::Client,
  //   );
  //   // wayland_scanner::generate_interfaces(
  //   //     protocol.0,
  //   //     output_dir.join(format!("{}_interfaces.rs", protocol.1))
  //   // );
  // }
}

fn link_optional_libs() {
  if cfg!(feature = "libcap") && pkg_config::probe_library("libcap").is_ok() {
    println!("cargo:rustc-link-lib=dylib=cap");
  }
  if cfg!(feature = "systemd") && pkg_config::probe_library("libsystemd").is_ok() {
    println!("cargo:rustc-link-lib=dylib=systemd");
  }
  if cfg!(feature = "elogind") && pkg_config::probe_library("elogind").is_ok() {
    println!("cargo:rustc-link-lib=dylib=elogind");
  }
  if pkg_config::probe_library("xcb-errors").is_ok() {
    println!("cargo:rustc-link-lib=dylib=xcb-errors");
  }
}
