use std::path::{Path, PathBuf};

use ecow::{EcoString, eco_format};
use typst::diag::FileResult;
use typst::foundations::{Bytes, Datetime, Dict, Duration, IntoValue};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_kit::datetime::Time;
use typst_kit::diagnostics::DiagnosticWorld;
use typst_kit::downloader::SystemDownloader;
use typst_kit::files::{FileStore, FsRoot, SystemFiles};
use typst_kit::fonts::{self, FontStore};
use typst_kit::packages::SystemPackages;

use crate::args::Args;

/// A world backed by the local file system.
pub struct DocxWorld {
    /// The current working directory, for diagnostic display.
    workdir: Option<PathBuf>,
    /// The id of the main source file.
    main: FileId,
    /// Typst's standard library.
    library: LazyHash<Library>,
    /// Discovered fonts.
    fonts: FontStore,
    /// Maps file ids to source files and buffers.
    files: FileStore<SystemFiles>,
    /// The fixed datetime of this compilation.
    now: Time,
}

impl DocxWorld {
    pub fn new(args: &Args) -> Result<Self, EcoString> {
        let input = args.input.canonicalize().map_err(|err| {
            eco_format!("input file not found: {} ({err})", args.input.display())
        })?;

        let root = match &args.root {
            Some(root) => root.clone(),
            None => input.parent().unwrap_or(Path::new(".")).to_path_buf(),
        };
        let root = root.canonicalize().map_err(|err| {
            eco_format!("root directory not found: {} ({err})", root.display())
        })?;

        let vpath = VirtualPath::virtualize(&root, &input).map_err(|err| {
            eco_format!("input file must be contained in the project root: {err}")
        })?;
        let main = RootedPath::new(VirtualRoot::Project, vpath).intern();

        let inputs: Dict = args
            .inputs
            .iter()
            .map(|(key, value)| (key.as_str().into(), value.as_str().into_value()))
            .collect();
        let library = Library::builder().with_inputs(inputs).build();

        let mut font_store = FontStore::new();
        if !args.ignore_system_fonts {
            font_store.extend(fonts::system());
        }
        font_store.extend(fonts::embedded());
        for path in &args.font_paths {
            font_store.extend(fonts::scan(path));
        }

        let downloader =
            SystemDownloader::new(concat!("typst-docx/", env!("CARGO_PKG_VERSION")));
        let packages = SystemPackages::new(downloader);
        let files = FileStore::new(SystemFiles::new(FsRoot::new(root), packages));

        Ok(Self {
            workdir: std::env::current_dir().ok(),
            main,
            library: LazyHash::new(library),
            fonts: font_store,
            files,
            now: Time::system(),
        })
    }
}

impl World for DocxWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }

    fn main(&self) -> FileId {
        self.main
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.source(id)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.file(id)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }

    fn today(&self, offset: Option<Duration>) -> Option<Datetime> {
        self.now.today(offset)
    }
}

impl DiagnosticWorld for DocxWorld {
    fn name(&self, id: FileId) -> String {
        let vpath = id.vpath();
        match id.root() {
            VirtualRoot::Project => {
                // Prefer a path relative to the working directory.
                self.files
                    .loader()
                    .resolve(id)
                    .ok()
                    .map(|path| {
                        self.workdir
                            .as_deref()
                            .and_then(|dir| path.strip_prefix(dir).ok())
                            .unwrap_or(&path)
                            .display()
                            .to_string()
                    })
                    .unwrap_or_else(|| vpath.get_without_slash().into())
            }
            VirtualRoot::Package(package) => {
                format!("{package}{}", vpath.get_with_slash())
            }
        }
    }
}
