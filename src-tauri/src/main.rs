fn main() {
    if let Err(error) = reconsile_lib::run() {
        eprintln!("Reconsile failed: {error}");
        std::process::exit(1);
    }
}
