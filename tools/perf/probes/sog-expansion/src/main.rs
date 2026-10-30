use std::{collections::HashMap, fs, hint::black_box, io::{Cursor,Read}, time::Instant};
use sha2::{Digest,Sha256};
use sog_codec::expanded::{parse_meta,decode_expanded,Expanded};
fn digest(cloud:&Expanded)->String {
    let mut hash=Sha256::new();
    for plane in [&cloud.positions,&cloud.rotations,&cloud.scales,&cloud.harmonics] {
        hash.update((plane.len() as u64).to_le_bytes());
        for value in plane {hash.update(value.to_bits().to_le_bytes());}
    }
    format!("{:x}",hash.finalize())
}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let path=std::env::args_os().nth(1).ok_or("missing source")?;
    let bytes=fs::read(path)?;
    let mut archive=zip::ZipArchive::new(Cursor::new(&bytes))?;
    let mut all=HashMap::new();
    for index in 0..archive.len(){
        let mut entry=archive.by_index(index)?;
        if entry.is_dir(){continue;}
        let name=entry.name().rsplit('/').next().unwrap().to_owned();
        let mut data=Vec::new();entry.read_to_end(&mut data)?;all.insert(name,data);
    }
    let meta=parse_meta(&all["meta.json"])?;
    let files:HashMap<_,_>=meta.required_files_for_decode(16).into_iter().map(|name|(name.to_owned(),all[name].clone())).collect();
    drop(all);drop(archive);drop(bytes);
    // Archive reading, metadata parsing, input validation and output hashing are outside timing.
    let mut samples=Vec::new();let mut first_hash=None;let mut last_hash=String::new();
    for iteration in 0..17 {
        let started=Instant::now();
        let cloud=black_box(decode_expanded(black_box(&meta),black_box(&files),16)?);
        let elapsed=started.elapsed().as_nanos() as u64;
        if iteration==0 {first_hash=Some(digest(&cloud));}
        if iteration>=2 {samples.push(elapsed);}
        if iteration==16 {last_hash=digest(&cloud);}
        drop(cloud); // destructor excluded, as in the production decode-stage timer
    }
    assert_eq!(first_hash.as_ref().unwrap(),&last_hash);
    println!("{}",serde_json::json!({"schema":"sog-expanded-diagnostic/1","count":meta.count,"coefficients_per_channel":16,"warmup_iterations":2,"samples_ns":samples,"expanded_sha256":last_hash,"scope":"native shared expansion including WebP and output allocation; pre-extracted resident encoded inputs; excludes ZIP, disk, metadata, output drop, renderer, GPU and worker startup; repeated iterations are not independent processes"}));
    Ok(())
}
