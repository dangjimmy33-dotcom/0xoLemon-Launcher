use first_light_launcher::overlay_injector;
use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: injector_cli <pid> <dll_dir>");
        std::process::exit(1);
    }

    let pid: u32 = args[1].parse().expect("Invalid PID");
    let dll_dir = PathBuf::from(&args[2]);

    println!("Injecting into PID: {}", pid);
    println!("DLL Directory: {}", dll_dir.display());

    match overlay_injector::inject(pid, &dll_dir) {
        Ok(_) => println!("Injection successful!"),
        Err(e) => eprintln!("Injection failed: {}", e),
    }
}
