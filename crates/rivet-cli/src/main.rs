//! `rivet-cli`: a small command-line front end to rivet-core.
//! Handy for testing the engine without starting the app.

use std::{fs::File, io::BufWriter, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rivet_core::Pdf;

#[derive(Parser)]
#[command(version, about = "Rivet PDF command-line tool")]
struct Cli {
    /// Folder containing the PDFium library (default: vendor/pdfium/<platform>).
    #[arg(long, global = true, env = "RIVET_PDFIUM_DIR")]
    pdfium: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print basic information about a PDF.
    Info { file: PathBuf },
    /// Render one page (1-based) to a PNG image.
    /// Write a test PDF with N numbered pages (for performance testing).
    MakeTestPdf {
        output: PathBuf,
        #[arg(long, default_value_t = 500)]
        pages: u32,
    },
    Render {
        file: PathBuf,
        page: u32,
        output: PathBuf,
        /// 1.0 = 72 DPI, 2.0 = 144 DPI, ...
        #[arg(long, default_value_t = 2.0)]
        scale: f32,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let pdfium_dir = cli.pdfium.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../vendor/pdfium")
            .join(rivet_core::pdfium_platform())
    });
    let pdf = Pdf::load(&pdfium_dir)?;

    match cli.command {
        Command::Info { file } => {
            let info = pdf.open(&file, None)?.info()?;
            println!("Title:  {}", info.title.as_deref().unwrap_or("-"));
            println!("Author: {}", info.author.as_deref().unwrap_or("-"));
            println!("Pages:  {}", info.page_count);
            for (i, size) in info.page_sizes.iter().enumerate() {
                println!(
                    "  page {:>4}: {:.0} × {:.0} pt",
                    i + 1,
                    size.width,
                    size.height
                );
            }
        }
        Command::MakeTestPdf { output, pages } => {
            let start = std::time::Instant::now();
            pdf.write_test_document(&output, pages)?;
            println!(
                "Wrote {} ({pages} pages) in {:?}",
                output.display(),
                start.elapsed()
            );
        }
        Command::Render {
            file,
            page,
            output,
            scale,
        } => {
            let index = page.checked_sub(1).context("pages are numbered from 1")?;
            let rendered =
                pdf.open(&file, None)?
                    .render_page(index, scale, rivet_core::Rotation::None)?;
            let mut encoder = png::Encoder::new(
                BufWriter::new(File::create(&output)?),
                rendered.width,
                rendered.height,
            );
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.write_header()?.write_image_data(&rendered.rgba)?;
            println!(
                "Wrote {} ({}×{} px)",
                output.display(),
                rendered.width,
                rendered.height
            );
        }
    }
    Ok(())
}
