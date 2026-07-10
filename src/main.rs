fn main() {
    if let Err(error) = keystone_xdtl::runtime::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
