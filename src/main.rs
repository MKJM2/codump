use anyhow::{Context, Result};
use arboard::Clipboard;
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;
use phf::phf_map;
use rayon::prelude::*;
use std::{fs, thread, time::Duration};
use std::io::Read;
use log::{warn, debug};
use regex::Regex;
use walkdir::WalkDir;

static LANG_MAP: phf::Map<&'static str, &'static str> = phf_map! {
    "rs" => "rust",
    "go" => "go",
    "c" => "c",
    "cpp" => "cpp",
    "cc" => "cpp",
    "cxx" => "cpp",
    "h" => "c",
    "hpp" => "cpp",
    "hxx" => "cpp",
    "js" => "javascript",
    "ts" => "typescript",
    "jsx" => "jsx",
    "tsx" => "tsx",
    "html" => "html",
    "css" => "css",
    "scss" => "scss",
    "sass" => "scss",
    "less" => "less",
    "java" => "java",
    "kt" => "kotlin",
    "kts" => "kotlin",
    "scala" => "scala",
    "groovy" => "groovy",
    "py" => "python",
    "rb" => "ruby",
    "php" => "php",
    "cs" => "csharp",
    "swift" => "swift",
    "pl" => "perl",
    "pm" => "perl",
    "lua" => "lua",
    "ex" => "elixir",
    "exs" => "elixir",
    "elm" => "elm",
    "hs" => "haskell",
    "erl" => "erlang",
    "fs" => "fsharp",
    "sh" => "bash",
    "bash" => "bash",
    "zsh" => "bash",
    "fish" => "fish",
    "ps1" => "powershell",
    "json" => "json",
    "toml" => "toml",
    "yaml" => "yaml",
    "yml" => "yaml",
    "xml" => "xml",
    "ini" => "ini",
    "conf" => "conf",
    "properties" => "properties",
    "sql" => "sql",
    "graphql" => "graphql",
    "gql" => "graphql",
    "prisma" => "prisma",
    "md" => "markdown",
    "markdown" => "markdown",
    "rst" => "rst",
    "txt" => "text",
    "csv" => "csv",
    "org" => "org",
};

const DEFAULT_EXTENSIONS_STR: &str = concat!(
    "rs,py,js,ts,jsx,tsx,go,java,c,cpp,cc,cxx,",
    "h,hpp,hxx,cs,rb,php,scala,kt,kts,groovy,pl,pm,swift,lua,",
    "ex,exs,elm,hs,erl,fs,sh,bash,zsh,fish,ps1,json,toml,",
    "yaml,yml,xml,ini,conf,properties,sql,graphql,gql,prisma,",
    "md,markdown,rst,txt,csv,org,html,css,scss,sass,less,tex,",
    "rmd,bat"
);

const DEFAULT_EXCLUDES_STR: &str = concat!(
    ".git,node_modules,target,dist,build,venv,.venv,__pycache__,",
    ".idea,.vscode,bin,obj,.mypy_cache,debug,.fingerprint,.cache,",
    "bower_components,coverage,tmp,temp,.next,out,logs,release,",
    ".gradle,gradle,vendor,packages,artifacts,generated,pods,",
    ".eggs,.pytest_cache,cmake-build-debug,cmake-build-release,",
    "CMakeFiles,.vs,out,.ipynb_checkpoints"
);


#[derive(Parser, Debug)]
#[command(
    name = "dumpcode",
    about = "dumps project files in an llm-friendly format",
    version
)]
struct Cli {
    #[arg(default_value = ".", help = "directory to scan")]
    directory: String,

    #[arg(short, long, help = "copy output to clipboard")]
    clipboard: bool,

    #[arg(
        short,
        long,
        default_value = DEFAULT_EXTENSIONS_STR,
        help = "file extensions to include"
    )]
    extensions: String,

    #[arg(short, long, default_value_t = 100, help = "max file size in kb")]
    max_size: usize,

    #[arg(
        short = 'x',
        long,
        default_value = DEFAULT_EXCLUDES_STR,
        help = "directories to exclude"
    )]
    exclude: String,

    #[arg(long, default_value_t = 1000, help = "maximum files to include")]
    max_files: usize,

    #[arg(short, long, help = "enable debug logging")]
    verbose: bool,
}


fn main() -> Result<()> {
    let cli = Cli::parse();
    env_logger::Builder::new()
        .filter_level(if cli.verbose { log::LevelFilter::Debug } else { log::LevelFilter::Warn })
        .init();

    debug!(target: "dumpcode", "cli args: {:?}", cli);

    let extensions_vec = cli.extensions.split(',').map(|s| s.trim().to_lowercase()).collect::<Vec<_>>();
    let exclude_dirs_vec = cli.exclude.split(',').map(|s| s.trim()).collect::<Vec<_>>();

    let output = generate_dump(
        &cli.directory,
        &extensions_vec,
        cli.max_size,
        &exclude_dirs_vec,
        cli.max_files,
    )?;

    if cli.clipboard {
        set_clipboard(&output).context("failed to copy output to clipboard")?;
        println!("Code dump copied to clipboard");
    } else {
        println!("{}", output);
    }

    Ok(())
}

fn generate_dump(
    directory: &str,
    extensions: &[String],
    max_size_kb: usize,
    exclude_dirs: &[&str],
    max_files: usize,
) -> Result<String> {
    let mut output = String::new();
    let (tree, included_files) =
        generate_tree_view(directory, extensions, max_size_kb, exclude_dirs, max_files)?;
    output.push_str("# project structure\n\n");
    output.push_str(&tree);
    output.push_str("\n\n");

    let base = Utf8Path::new(directory);
    let files_output: Result<Vec<String>> = included_files
        .par_iter()
        .map(|relative_path| {
            let start_time = std::time::Instant::now();
            let full_path = base.join(relative_path);

            let mut file = fs::File::open(full_path.as_std_path())?;
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer)?;

            let content = match String::from_utf8(buffer) {
                Ok(s) => s,
                Err(e) => {
                    warn!("non-utf8 file skipped: {} ({})", relative_path, e);
                    return Ok(String::new());
                }
            };

            let ext = relative_path.extension().unwrap_or("").to_lowercase();
            let lang = language_for_extension(&ext, &content);
            debug!("processed {} in {:?}", relative_path, start_time.elapsed());
            let file_dump = format!(
                "# file: {}\n\n```{}\n{}\n```\n\n",
                relative_path, lang, content
            );
            Ok(file_dump)
        })
        .collect();

    for file_out in files_output? {
        output.push_str(&file_out);
    }

    Ok(output)
}

fn generate_tree_view(
    path: &str,
    extensions: &[String],
    max_size_kb: usize,
    exclude_dirs: &[&str],
    max_files: usize,
) -> Result<(String, Vec<Utf8PathBuf>)> {
    let mut file_count = 0;
    let mut tree = String::new();
    let mut files = Vec::new();

    let base = Utf8Path::new(path).file_name().unwrap_or(path);
    tree.push_str(&format!("{}/\n", base));

    let walker = WalkDir::new(path)
        .min_depth(1)
        .follow_links(false)
        .same_file_system(true)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !exclude_dirs.iter().any(|d| name == *d)
        });

    for entry in walker {
        let entry = entry?;
        if entry.path_is_symlink() {
            warn!("skipping symlink: {}", entry.path().display());
            continue;
        }

        let entry_path = entry.path();
        let file_name = entry.file_name().to_string_lossy();
        let is_excluded = entry.depth() > 0 && exclude_dirs.iter().any(|d| file_name == *d);
        if is_excluded {
            continue;
        }
        if file_count >= max_files {
            break;
        }

        let depth = entry.depth();
        let indent = "  ".repeat(depth - 1);
        let prefix = if depth == 1 { "├── " } else { "└── " };
        if entry.file_type().is_file() {
            let metadata = entry.metadata()?;
            let size_kb = metadata.len() / 1024;
            let ext = entry_path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if extensions.contains(&ext) && size_kb <= max_size_kb as u64 {
                file_count += 1;
                let rel_path = Utf8Path::from_path(entry_path)
                    .and_then(|p| p.strip_prefix(path).ok())
                    .unwrap_or_else(|| Utf8Path::from_path(entry_path).unwrap())
                    .to_owned();
                tree.push_str(&format!("{}{}{} [{}kb]\n", indent, prefix, rel_path, size_kb));
                files.push(rel_path.clone());
            }
        } else if entry.file_type().is_dir() {
            let rel_path = Utf8Path::from_path(entry_path)
                .and_then(|p| p.strip_prefix(path).ok())
                .unwrap_or_else(|| Utf8Path::from_path(entry_path).unwrap())
                .to_owned();
            tree.push_str(&format!("{}{}{}/\n", indent, prefix, rel_path));
        }
    }

    Ok((tree, files))
}

fn language_for_extension(ext: &str, content: &str) -> &'static str {
    LANG_MAP.get(ext).copied().unwrap_or_else(|| {
        if content.starts_with("#!") {
            detect_shebang(content)
        } else if ext.is_empty() {
            detect_special_file(content)
        } else {
            ""
        }
    })
}

fn detect_shebang(content: &str) -> &'static str {
    lazy_static::lazy_static! {
        static ref SHEBANG_RE: Regex = Regex::new(r"^#!\s*/usr/bin/env\s+(\w+)|^#!\s*/.*/(\w+)").unwrap();
    }

    if let Some(first_line) = content.lines().next() {
        if let Some(caps) = SHEBANG_RE.captures(first_line) {
            let lang = caps.get(1).or_else(|| caps.get(2)).map(|m| m.as_str());
            return match lang {
                Some("python3") | Some("python") => "python",
                Some("ruby") => "ruby",
                Some("node") | Some("nodejs") => "javascript",
                Some("bash") | Some("sh") => "bash",
                Some("perl") => "perl",
                Some("php") => "php",
                Some("lua") => "lua",
                Some("Rscript") => "r",
                _ => "",
            };
        }
    }
    ""
}

fn detect_special_file(content: &str) -> &'static str {
    if content.contains("FROM ") {
        "dockerfile"
    } else if content.contains("JAVA_HOME") {
        "properties"
    } else {
        ""
    }
}

fn set_clipboard(text: &str) -> Result<()> {
    let mut attempts = 0;
    let max_attempts = 3;

    while attempts < max_attempts {
        match Clipboard::new().and_then(|mut c| c.set_text(text)) {
            Ok(_) => return Ok(()),
            Err(e) if attempts == max_attempts - 1 => return Err(e.into()),
            Err(_) => {
                thread::sleep(Duration::from_millis(50));
                attempts += 1;
            }
        }
    }

    Ok(())
}
