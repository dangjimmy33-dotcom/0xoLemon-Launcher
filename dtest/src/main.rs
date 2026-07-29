use oxidelta::{Options, generate_delta_file, apply_delta_file};
use std::fs::File;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "source.txt";
    let target = "target.txt";
    let delta = "delta.bin";
    let restored = "restored.txt";

    // Create source
    std::fs::write(source, "hello world 123")?;
    // Create target
    std::fs::write(target, "hello world 456")?;

    let mut source_file = File::open(source)?;
    let mut target_file = File::open(target)?;
    let mut delta_file = File::create(delta)?;

    println!("Generating delta...");
    generate_delta_file(Options::default(), &mut source_file, &mut target_file, &mut delta_file)?;
    println!("Delta generated.");

    // Now apply delta
    let mut source_file2 = File::open(source)?;
    let mut delta_file2 = File::open(delta)?;
    let mut restored_file = File::create(restored)?;

    println!("Applying delta...");
    apply_delta_file(&mut source_file2, &mut delta_file2, &mut restored_file)?;
    println!("Delta applied.");

    let restored_content = std::fs::read_to_string(restored)?;
    println!("Restored: {}", restored_content);
    assert_eq!(restored_content, "hello world 456");

    Ok(())
}
