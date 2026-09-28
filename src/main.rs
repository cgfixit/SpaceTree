fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--scan") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| ".".into());
        match spacetree::scan(std::path::Path::new(&path)) {
            Ok(result) => print!("{}", spacetree::format_scan(&result)),
            Err(e) => {
                eprintln!("scan failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("SpaceTree — macOS disk usage analyzer");
        eprintln!("  spacetree --scan <path>       print expandable tree");
        eprintln!("  spacetree --gui-scan <path>   open the GUI and scan");
        eprintln!("  spacetree                     open the GUI");
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--gui-scan") {
        if let Some(path) = args.get(i + 1) {
            std::env::set_var("SPACETREE_AUTOSCAN", path);
        }
    }
    if let Err(e) = spacetree::app::run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
