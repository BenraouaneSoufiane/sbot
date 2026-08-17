fn main() {
    if let Err(error) = sbot_lib::run() {
        eprintln!("sbot failed: {error}");
        std::process::exit(1);
    }
}
