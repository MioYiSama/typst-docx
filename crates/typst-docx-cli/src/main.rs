mod args;
mod world;

use std::process::ExitCode;

use clap::Parser;
use ecow::{EcoString, eco_format};
use typst::diag::Warned;
use typst_kit::diagnostics::termcolor::{ColorChoice, StandardStream};
use typst_kit::diagnostics::{self, DiagnosticFormat};
use typst_layout::PagedDocument;

use crate::args::Args;
use crate::world::DocxWorld;

fn main() -> ExitCode {
    let args = Args::parse();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &Args) -> Result<(), EcoString> {
    let world = DocxWorld::new(args)?;
    let Warned { output, warnings } = typst::compile::<PagedDocument>(&world);

    let mut stderr = StandardStream::stderr(ColorChoice::Auto);
    let mut emit = |diagnostics| {
        diagnostics::emit(&mut stderr, &world, diagnostics, DiagnosticFormat::Human)
            .map_err(|err| eco_format!("failed to print diagnostics ({err})"))
    };
    emit(&warnings)?;

    let document = match output {
        Ok(document) => document,
        Err(errors) => {
            emit(&errors)?;
            return Err("compilation failed".into());
        }
    };

    let result = typst_docx::docx(&document);
    for warning in &result.warnings {
        eprintln!("warning: {warning}");
    }

    let output_path = args
        .output
        .clone()
        .unwrap_or_else(|| args.input.with_extension("docx"));
    std::fs::write(&output_path, result.bytes).map_err(|err| {
        eco_format!("failed to write {} ({err})", output_path.display())
    })?;

    Ok(())
}
