//! Renders a PDF's sheets to PNG files, to look at an export by eye.
//!
//! `cargo run -p notecraft-pdf --example pdf_png -- IN.pdf OUT_DIR [max side px]`

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let input = args.next().ok_or("usage: pdf_png IN.pdf OUT_DIR [max side px]")?;
    let out = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".into()));
    let side: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1400);
    std::fs::create_dir_all(&out)?;
    let pdf = std::fs::read(&input)?;
    let stem = std::path::Path::new(&input).file_stem().map_or_else(|| "page".into(), |s| s.to_string_lossy().into_owned());
    for (i, t) in notecraft_pdf::thumbnails(&pdf, side, 200).into_iter().enumerate() {
        let img = image::RgbaImage::from_raw(t.width, t.height, t.rgba).ok_or("bad raster")?;
        let path = out.join(format!("{stem}-{}.png", i + 1));
        img.save(&path)?;
        println!("{}", path.display());
    }
    Ok(())
}
