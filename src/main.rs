fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if let Some(i) = args.iter().position(|a| a == "--scan") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| ".".into());
        let depth = match args.iter().position(|a| a == "--depth") {
            Some(d) => args
                .get(d + 1)
                .and_then(|v| v.to_str())
                .and_then(|n| n.parse().ok())
                .unwrap_or_else(|| {
                    eprintln!("--depth requires a nonnegative integer");
                    std::process::exit(2)
                }),
            None => usize::MAX,
        };
        match spacetree::scan(std::path::Path::new(&path)) {
            Ok(result) => {
                print!("{}", spacetree::format_scan_to_depth(&result, depth));
                if result.error_count > 0 {
                    eprintln!(
                        "scan incomplete: {} read {}; {}",
                        result.error_count,
                        if result.error_count == 1 {
                            "error"
                        } else {
                            "errors"
                        },
                        if result.volume_used.is_some() {
                            "what could not be read is counted in Hidden space"
                        } else {
                            "sizes are partial"
                        }
                    );
                    eprint!("{}", spacetree::format_refusals(&result));
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("scan failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "--volumes") {
        print!(
            "{}",
            spacetree::format_volumes(&spacetree::mounted_volumes())
        );
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--volume") {
        let path = args.get(i + 1).cloned().unwrap_or_else(|| "/".into());
        match spacetree::try_format_volume(std::path::Path::new(&path)) {
            Ok(text) => print!("{text}"),
            Err(e) => {
                eprintln!("volume lookup failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(i) = args.iter().position(|a| a == "--ollama") {
        let dir = match args.get(i + 1) {
            Some(dir) => Some(std::path::PathBuf::from(dir)),
            None => spacetree::default_models_dir(),
        };
        let report = dir
            .ok_or_else(|| "no models directory: set OLLAMA_MODELS or pass a path".to_string())
            .and_then(|dir| spacetree::ollama_report(&dir));
        match report {
            Ok((text, complete)) => {
                print!("{text}");
                if !complete {
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("ollama report failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|a| a == "-h" || a == "--help") {
        eprintln!("SpaceTree — macOS disk usage analyzer");
        eprintln!("  spacetree --scan <path>       print expandable tree");
        eprintln!("      --depth <n>               print rows no deeper than n below the root");
        eprintln!("  spacetree --volume [path]     print the volume's size figures, no scan");
        eprintln!("  spacetree --volumes           list mounted disks and network shares to scan");
        eprintln!("  spacetree --ollama [dir]      list Ollama models with their blobs, read-only");
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
