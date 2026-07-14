#[cfg(windows)]
fn main() {
    let code = terax_lib::modules::terminal_control::cli::run(std::env::args().skip(1));
    std::process::exit(code);
}

#[cfg(not(windows))]
fn main() {
    eprintln!("teraxctl is available only in Windows-native Terax terminals");
    std::process::exit(3);
}
