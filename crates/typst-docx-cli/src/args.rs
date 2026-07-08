use std::path::PathBuf;

use clap::Parser;

/// Compiles a Typst file into a visually faithful DOCX document.
///
/// The output places every page element at its exact position, so it looks
/// like the PDF output but is not meant to be edited. Requires Word 2013 or
/// later to display.
#[derive(Debug, Parser)]
#[command(name = "typst-docx", version)]
pub struct Args {
    /// Path to the input Typst file.
    pub input: PathBuf,

    /// Path to the output DOCX file. Defaults to the input with the
    /// extension replaced by `.docx`.
    #[arg(short, long, value_name = "OUTPUT.docx")]
    pub output: Option<PathBuf>,

    /// Configures the project root (for absolute paths).
    #[arg(long, env = "TYPST_ROOT", value_name = "DIR")]
    pub root: Option<PathBuf>,

    /// Adds additional directories that are recursively searched for fonts.
    #[arg(
        long = "font-path",
        env = "TYPST_FONT_PATHS",
        value_name = "DIR",
        value_delimiter = ':'
    )]
    pub font_paths: Vec<PathBuf>,

    /// Ensures system fonts won't be searched.
    #[arg(long)]
    pub ignore_system_fonts: bool,

    /// Add a string key-value pair visible through `sys.inputs`.
    #[arg(long = "input", value_name = "key=value", value_parser = parse_input_pair)]
    pub inputs: Vec<(String, String)>,
}

/// Parses a `key=value` pair for `--input`.
fn parse_input_pair(raw: &str) -> Result<(String, String), String> {
    let (key, value) = raw
        .split_once('=')
        .ok_or("input must be a key and a value separated by an equal sign")?;
    let key = key.trim().to_owned();
    if key.is_empty() {
        return Err("the key was missing or empty".into());
    }
    Ok((key, value.trim().to_owned()))
}
