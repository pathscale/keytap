//! keytap: driverless macOS key remapper via a CoreGraphics event tap.
//!
//! Assumes the macOS Modifier Keys swap (Control <-> Command) stays ON.
//! See README for design and install.

mod config;
mod ffi;
mod tap;

use keytap_rules::Engine;
use std::process::ExitCode;

fn usage() -> ! {
    eprintln!(
        "\
keytap - driverless Control+Tab → Command+Tab remapper

Usage:
  keytap                 run the event tap (needs Accessibility)
  keytap --print-config  print resolved config path and contents
  keytap --write-config  write the default config if missing
  keytap --dry-run       create the tap and log actions (still needs Accessibility)
  keytap --check-ffi     call AXIsProcessTrustedWithOptions(prompt=false); print result
  keytap --help

Environment:
  KEYTAP_CONFIG   override config file path
"
    );
    std::process::exit(2);
}

fn check_accessibility(prompt: bool) -> bool {
    unsafe { ffi::ax_is_process_trusted(prompt) }
}

fn main() -> ExitCode {
    let mut print_config = false;
    let mut write_config = false;
    let mut dry_run = false;
    let mut check_ffi = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--help" | "-h" => usage(),
            "--print-config" => print_config = true,
            "--write-config" => write_config = true,
            "--dry-run" => dry_run = true,
            "--check-ffi" => check_ffi = true,
            other => {
                eprintln!("unknown argument `{other}`");
                usage();
            }
        }
    }

    if check_ffi {
        // Exercises the options dictionary path with prompt=false. Must not crash.
        let trusted = unsafe { ffi::ax_is_process_trusted(false) };
        println!("trusted {}", if trusted { "true" } else { "false" });
        return ExitCode::SUCCESS;
    }

    let (path, cfg) = match config::load_or_default() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("keytap: config error: {e}");
            return ExitCode::from(1);
        }
    };

    if print_config {
        println!("path: {}", path.display());
        println!("exists: {}", path.exists());
        print!("{}", cfg.render());
        return ExitCode::SUCCESS;
    }

    if write_config {
        if path.exists() {
            println!("config already exists at {}", path.display());
        } else {
            match config::write_default(&path) {
                Ok(()) => println!("wrote default config to {}", path.display()),
                Err(e) => {
                    eprintln!("write {}: {e}", path.display());
                    return ExitCode::from(1);
                }
            }
        }
        return ExitCode::SUCCESS;
    }

    if !check_accessibility(true) {
        eprintln!(
            "keytap: Accessibility permission is required.\n\
             Open System Settings → Privacy & Security → Accessibility\n\
             and enable keytap (or the terminal you launched it from).\n\
             Then run keytap again."
        );
        return ExitCode::from(1);
    }

    if dry_run {
        eprintln!(
            "keytap: dry-run mode (actions logged, still posts nothing synthetic as suppress+log)"
        );
    }

    let engine = Engine::new(cfg);
    let tap = match tap::Tap::create(engine, dry_run) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("keytap: {e}");
            return ExitCode::from(1);
        }
    };
    tap.run();
    ExitCode::SUCCESS
}
