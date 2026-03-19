fn main() {
    if let Err(err) = rupass::run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
