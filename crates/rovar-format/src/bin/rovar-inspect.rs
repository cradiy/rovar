use anyhow::{Context, Result, ensure};
use rovar_format::Reader;
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .context("Usage: rovar-inspect FILE [--verify]")?;
    let verify = match args.next().as_deref() {
        None => false,
        Some("--verify") => true,
        _ => anyhow::bail!("Unknown option"),
    };
    ensure!(args.next().is_none(), "Too many arguments");
    let reader = Reader::open(path)?;
    println!(
        "format={} generation={} committed_bytes={}",
        reader.version(),
        reader.generation(),
        reader.committed_length()
    );
    for (key, block) in reader.entries() {
        println!(
            "{}\t{}\t{}\t{}",
            key, block.kind, block.offset, block.length
        );
    }
    if verify {
        reader.verify()?;
        println!("All blocks verified");
    }
    Ok(())
}
