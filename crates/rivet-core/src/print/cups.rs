//! Printing on Linux and macOS through CUPS (the standard printing system on both).
//!
//! Printers are listed with `lpstat` and `lpoptions`; a job is a PDF of the
//! chosen pages handed to `lp` with the settings as CUPS options. CUPS then
//! takes care of scaling, two-sided printing and the printer's margins.

use std::{io::Write, process::Command};

use pdfium_render::prelude::*;

use super::{Duplex, Orientation, Paper, PrintSettings, PrinterInfo, Scaling};
use crate::{Error, ErrorCode, Result};

pub(crate) fn list_printers() -> Result<Vec<PrinterInfo>> {
    // No CUPS tools (or no printers) simply means an empty list.
    let Some(names) = run("lpstat", &["-e"]) else {
        return Ok(Vec::new());
    };
    let default = run("lpstat", &["-d"]).and_then(|out| parse_default(&out));
    Ok(names
        .lines()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(|name| {
            let options = run("lpoptions", &["-p", name, "-l"]).unwrap_or_default();
            printer_from_options(name, default.as_deref() == Some(name), &options)
        })
        .collect())
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `lpstat -d` prints "system default destination: NAME".
fn parse_default(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|l| l.split_once("destination:"))
        .map(|(_, name)| name.trim().to_owned())
        .filter(|n| !n.is_empty())
}

/// Builds printer info from `lpoptions -p NAME -l`, whose lines look like
/// `PageSize/Media Size: Letter *A4 Legal` (the `*` marks the current choice).
fn printer_from_options(name: &str, is_default: bool, options: &str) -> PrinterInfo {
    let choices = |key: &str| -> Vec<String> {
        options
            .lines()
            .find(|l| l.split(['/', ':']).next() == Some(key))
            .and_then(|l| l.split_once(':'))
            .map(|(_, values)| values.split_whitespace().map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let sizes = choices("PageSize");
    let papers: Vec<Paper> = sizes
        .iter()
        .filter_map(|s| paper_from_keyword(s.trim_start_matches('*')))
        .collect();
    let default_paper = sizes
        .iter()
        .find(|s| s.starts_with('*'))
        .map(|s| s.trim_start_matches('*').to_owned());
    let duplex = choices("Duplex")
        .iter()
        .any(|c| c.trim_start_matches('*') != "None");
    let color_model = choices("ColorModel");
    let color = color_model.is_empty()
        || color_model
            .iter()
            .any(|c| !c.trim_start_matches('*').eq_ignore_ascii_case("gray"));
    PrinterInfo {
        name: name.to_owned(),
        is_default,
        papers: if papers.is_empty() {
            common_papers()
        } else {
            papers
        },
        default_paper,
        duplex,
        color,
        has_properties: false,
    }
}

/// Paper sizes for CUPS keywords such as `A4`, `Letter` or `w595h842`.
fn paper_from_keyword(keyword: &str) -> Option<Paper> {
    let mm = |w: f32, h: f32, name: &str| Paper {
        id: keyword.to_owned(),
        name: name.to_owned(),
        width_mm: w,
        height_mm: h,
    };
    let known = match keyword {
        "A3" => mm(297.0, 420.0, "A3"),
        "A4" => mm(210.0, 297.0, "A4"),
        "A5" => mm(148.0, 210.0, "A5"),
        "A6" => mm(105.0, 148.0, "A6"),
        "B4" => mm(250.0, 353.0, "B4"),
        "B5" => mm(176.0, 250.0, "B5"),
        "Letter" => mm(215.9, 279.4, "Letter"),
        "Legal" => mm(215.9, 355.6, "Legal"),
        "Executive" => mm(184.2, 266.7, "Executive"),
        "Tabloid" => mm(279.4, 431.8, "Tabloid"),
        _ => {
            // Custom sizes in points: w<width>h<height>.
            let rest = keyword.strip_prefix('w')?;
            let (w, h) = rest.split_once('h')?;
            let (w, h): (f32, f32) = (w.parse().ok()?, h.parse().ok()?);
            let to_mm = 25.4 / 72.0;
            return Some(mm(
                w * to_mm,
                h * to_mm,
                &format!("{:.0} × {:.0} mm", w * to_mm, h * to_mm),
            ));
        }
    };
    Some(known)
}

fn common_papers() -> Vec<Paper> {
    ["A4", "Letter", "A3", "A5", "Legal"]
        .iter()
        .filter_map(|k| paper_from_keyword(k))
        .collect()
}

/// Turns print settings into `lp` arguments (everything except the file).
fn lp_arguments(settings: &PrintSettings) -> Vec<String> {
    let mut args = vec![
        "-d".into(),
        settings.printer.clone(),
        "-t".into(),
        settings.title.clone(),
        "-n".into(),
        settings.copies.max(1).to_string(),
    ];
    let mut option = |value: String| {
        args.push("-o".into());
        args.push(value);
    };
    if settings.copies > 1 {
        option(format!("collate={}", settings.collate));
    }
    if let Some(paper) = &settings.paper {
        option(format!("media={paper}"));
    }
    option(
        match settings.duplex {
            Duplex::OneSided => "sides=one-sided",
            Duplex::LongEdge => "sides=two-sided-long-edge",
            Duplex::ShortEdge => "sides=two-sided-short-edge",
        }
        .into(),
    );
    if settings.grayscale {
        option("print-color-mode=monochrome".into());
    }
    match settings.orientation {
        Orientation::Auto => {}
        Orientation::Portrait => option("orientation-requested=3".into()),
        Orientation::Landscape => option("orientation-requested=4".into()),
    }
    option(match settings.scaling {
        Scaling::Fit => "print-scaling=fit".into(),
        Scaling::Actual => "print-scaling=none".into(),
        Scaling::Shrink => "print-scaling=auto".into(),
        Scaling::Custom { percent } => format!("natural-scaling={}", percent.round() as i32),
    });
    args
}

pub(crate) fn print_document(
    pdfium: &Pdfium,
    document: &PdfDocument,
    settings: &PrintSettings,
    _printer_settings: Option<&[u8]>,
) -> Result<()> {
    let failed = |e: &dyn std::fmt::Display| Error::new(ErrorCode::PrintFailed, e.to_string());

    // A PDF with just the chosen pages, in the chosen order.
    let mut job = pdfium.create_new_pdf()?;
    for &page in &settings.pages {
        let index = page as PdfPageIndex;
        let at = job.pages().len();
        job.pages_mut()
            .copy_page_range_from_document(document, index..=index, at)?;
    }
    let bytes = job.save_to_bytes()?;

    let mut file = tempfile_in_temp_dir().map_err(|e| failed(&e))?;
    file.1.write_all(&bytes).map_err(|e| failed(&e))?;
    drop(file.1);

    let mut args = lp_arguments(settings);
    args.push(file.0.display().to_string());
    let output = Command::new("lp").args(&args).output();
    // lp copies the file into the print queue, so it can go right away.
    let _ = std::fs::remove_file(&file.0);
    let output = output.map_err(|e| failed(&e))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(failed(&String::from_utf8_lossy(&output.stderr).trim()))
    }
}

fn tempfile_in_temp_dir() -> std::io::Result<(std::path::PathBuf, std::fs::File)> {
    let path = std::env::temp_dir().join(format!(
        "rivet-print-{}-{}.pdf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default()
    ));
    let file = std::fs::File::create(&path)?;
    Ok((path, file))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTIONS: &str = "\
PageSize/Media Size: Letter *A4 Legal w288h432
Duplex/2-Sided Printing: *None DuplexNoTumble DuplexTumble
ColorModel/Color Mode: Gray *RGB
";

    #[test]
    fn reads_printer_options() {
        let p = printer_from_options("HP", true, OPTIONS);
        assert!(p.is_default && p.duplex && p.color);
        assert_eq!(p.default_paper.as_deref(), Some("A4"));
        let ids: Vec<_> = p.papers.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["Letter", "A4", "Legal", "w288h432"]);
        assert!((p.papers[3].width_mm - 101.6).abs() < 0.1);
    }

    #[test]
    fn handles_simple_printers() {
        let p = printer_from_options("Basic", false, "ColorModel/Color Mode: *Gray\n");
        assert!(!p.duplex && !p.color);
        assert!(!p.papers.is_empty(), "falls back to common paper sizes");
    }

    #[test]
    fn reads_the_default_printer() {
        assert_eq!(
            parse_default("system default destination: HP_Smart\n").as_deref(),
            Some("HP_Smart")
        );
        assert_eq!(parse_default("no system default destination\n"), None);
    }

    #[test]
    fn builds_lp_arguments() {
        let settings = PrintSettings {
            printer: "HP".into(),
            pages: vec![0],
            copies: 2,
            collate: true,
            scaling: Scaling::Custom { percent: 90.0 },
            orientation: Orientation::Landscape,
            duplex: Duplex::LongEdge,
            grayscale: true,
            paper: Some("A4".into()),
            title: "Doc".into(),
        };
        let args = lp_arguments(&settings).join(" ");
        for expected in [
            "-d HP",
            "-n 2",
            "collate=true",
            "media=A4",
            "sides=two-sided-long-edge",
            "print-color-mode=monochrome",
            "orientation-requested=4",
            "natural-scaling=90",
        ] {
            assert!(args.contains(expected), "missing {expected} in {args}");
        }
    }
}
