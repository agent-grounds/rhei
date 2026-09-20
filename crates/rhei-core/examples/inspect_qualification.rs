//! Offline evidence inspection only: no provider or subprocess launch.
//! §AR-neural-admission.8

use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 5 {
        return Err("usage: inspect_qualification BUNDLE SHA256 RAW_PUBLIC_KEY RAW_SIGNATURE ARTIFACT_DIRECTORY".into());
    }
    let payload = std::fs::read(&args[0])?;
    let expected_hash = args[1].to_str().ok_or("hash is not UTF-8")?;
    let key: [u8; 32] = std::fs::read(&args[2])?.try_into().map_err(|_| "key must be 32 bytes")?;
    let signature: [u8; 64] =
        std::fs::read(&args[3])?.try_into().map_err(|_| "signature must be 64 bytes")?;
    let report = rhei_core::budget::inspect_qualification_bundle(
        &payload,
        expected_hash,
        &key,
        &signature,
        |hash| {
            // The verifier has already checked the exact lowercase hash form.
            let path = Path::new(&args[4]).join(hash.trim_start_matches("sha256:"));
            Ok(std::fs::read(path)?)
        },
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    eprintln!("Verified signed evidence only; the release registry was not changed.");
    Ok(())
}
