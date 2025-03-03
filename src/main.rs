use anyhow::{Context, Result};
use arboard::Clipboard;
use clap::{Arg, Command};
use std::fs;
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let matches = Command::new("dump")
        .about("Dumps project files in an LLM-friendly format")
        .arg(
            Arg::new("directory")
                .help("Directory to scan (defaults to current)")
                .default_value(".")
                .index(1),
        )
        .arg(
            Arg::new("clipboard")
                .short('c')
                .long("clipboard")
                .help("Copy output to clipboard instead of stdout")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("extensions")
                .short('e')
                .long("extensions")
                .help("File extensions to include (comma-separated)")
                .default_value("rs,py,js,ts,jsx,tsx,go,java,c,cpp,h,hpp,cs,rb,php,scala,kt,pl,sh,bash,zsh,fish,json,toml,yaml,yml,md,txt,csv,xml,sql,graphql,prisma,html,css,scss,sass,less"),
        )
        .arg(
            Arg::new("max-size")
                .short('s')
                .long("max-size")
                .help("Max file size in KB to include")
                .default_value("100")
                .value_parser(clap::value_parser!(usize)),
        )
        .get_matches();

    let directory = matches.get_one::<String>("directory").unwrap();
    let to_clipboard = matches.get_flag("clipboard");
    let extensions = matches
        .get_one::<String>("extensions")
        .unwrap()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .collect::<Vec<_>>();
    let max_size_kb = *matches.get_one::<usize>("max-size").unwrap();

    let output = generate_dump(directory, &extensions, max_size_kb)?;

    if to_clipboard {
        let mut clipboard = Clipboard::new().context("failed to access clipboard")?;
        clipboard
            .set_text(&output)
            .context("failed to copy to clipboard")?;
        println!("project dump copied to clipboard");
    } else {
        println!("{}", output);
    }

    Ok(())
}

fn generate_dump(
    directory: &str,
    extensions: &[String],
    max_size_kb: usize,
) -> Result<String> {
    let path = Path::new(directory);
    let mut output = String::new();

    // Generate tree view and get list of files displayed in the tree
    let (tree, included_files) = generate_tree_view(path, extensions, max_size_kb)?;

    output.push_str("# Project Structure\n\n");
    output.push_str(&tree);
    output.push_str("\n\n");

    // Only dump files that were included in the tree
    for file_path in included_files {
        if let Ok(content) = fs::read_to_string(&file_path) {
            let rel_path = file_path
                .strip_prefix(path)
                .unwrap_or(&file_path)
                .to_string_lossy();

            output.push_str(&format!("# File: {}\n\n", rel_path));

            // Get file extension for language detection
            let lang = if let Some(ext) = file_path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                language_for_extension(&ext_str)
            } else {
                // Try to detect by filename
                let filename = file_path.file_name().map(|f| f.to_string_lossy().to_lowercase());
                match filename.as_deref() {
                    Some("makefile") => "makefile",
                    Some("dockerfile") => "dockerfile",
                    Some(".gitignore") => "gitignore",
                    Some(".dockerignore") => "gitignore", // Similar enough to gitignore
                    _ => "",
                }
            };

            output.push_str(&format!("```{}\n{}\n```\n\n", lang, content));
        }
    }

    Ok(output)
}

fn language_for_extension(ext: &str) -> &'static str {
    match ext {
        // Systems programming
        "rs" => "rust",
        "go" => "go",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" => "c",
        "hpp" | "hxx" => "cpp",

        // Web/JS related
        "js" => "javascript",
        "ts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "html" => "html",
        "css" => "css",
        "scss" | "sass" => "scss",
        "less" => "less",

        // JVM languages
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "scala" => "scala",
        "groovy" => "groovy",

        // Other languages
        "py" => "python",
        "rb" => "ruby",
        "php" => "php",
        "cs" => "csharp",
        "swift" => "swift",
        "pl" | "pm" => "perl",
        "lua" => "lua",
        "ex" | "exs" => "elixir",
        "elm" => "elm",
        "hs" => "haskell",
        "erl" => "erlang",
        "fs" => "fsharp",

        // Shell/scripts
        "sh" => "bash",
        "bash" => "bash",
        "zsh" => "bash",
        "fish" => "fish",
        "ps1" => "powershell",

        // Config files
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "xml" => "xml",
        "ini" => "ini",
        "conf" => "conf",
        "properties" => "properties",

        // Database/query languages
        "sql" => "sql",
        "graphql" | "gql" => "graphql",
        "prisma" => "prisma",

        // Markup/docs
        "md" | "markdown" => "markdown",
        "rst" => "rst",
        "txt" => "text",
        "csv" => "csv",
        "org" => "org",

        // Default - empty string means no highlighting
        _ => "",
    }
}

fn generate_tree_view(
    path: &Path,
    extensions: &[String],
    max_size_kb: usize
) -> Result<(String, Vec<PathBuf>)> {
    let mut result = String::new();
    let mut included_files = Vec::new();

    let base_name = path.file_name().unwrap_or_else(|| std::ffi::OsStr::new("."));
    result.push_str(&format!("{}/\n", base_name.to_string_lossy()));

    generate_tree_recursive(path, &mut result, "", true, extensions, max_size_kb, &mut included_files)?;

    Ok((result, included_files))
}

fn generate_tree_recursive(
    path: &Path,
    result: &mut String,
    prefix: &str,
    is_last: bool,
    extensions: &[String],
    max_size_kb: usize,
    included_files: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries_result = fs::read_dir(path);

    let mut entries = match entries_result {
        Ok(entries) => entries.filter_map(Result::ok).collect::<Vec<_>>(),
        Err(_) => return Ok(()),  // Skip directories we can't read
    };

    // Sort entries: directories first, then files
    entries.sort_by(|a, b| {
        let a_is_dir = a.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let b_is_dir = b.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        match (a_is_dir, b_is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.file_name().cmp(&b.file_name()),
        }
    });

    // Skip some common directories we don't want to include
    entries.retain(|entry| {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let skip_dirs = [
            ".git", "node_modules", "target", "dist", "build", "venv",
            "__pycache__", ".idea", ".vscode", "bin", "obj", ".mypy_cache"
        ];

        !(entry.file_type().map_or(false, |ft| ft.is_dir()) &&
          skip_dirs.contains(&name_str.as_ref()))
    });

    for (i, entry) in entries.iter().enumerate() {
        let is_entry_last = i == entries.len() - 1;
        let connector = if is_entry_last { "└── " } else { "├── " };
        let next_prefix = if is_entry_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        let entry_path = entry.path();
        let is_dir = entry.file_type()?.is_dir();

        // For files, check if we should include them based on extension and size
        if !is_dir {
            let include = if let Some(ext) = entry_path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                let size_ok = match fs::metadata(&entry_path) {
                    Ok(metadata) => (metadata.len() as usize / 1024) <= max_size_kb,
                    Err(_) => false,
                };

                size_ok && extensions.contains(&ext_str)
            } else {
                // Special case for files without extensions like Makefile, Dockerfile, etc.
                let name = entry_path.file_name().map(|f| f.to_string_lossy().to_lowercase());
                matches!(name.as_deref(), Some("makefile" | "dockerfile" | ".gitignore" | ".dockerignore"))
            };

            if include {
                result.push_str(&format!(
                    "{}{}{}",
                    prefix,
                    connector,
                    entry.file_name().to_string_lossy()
                ));
                result.push('\n');

                // Add to our list of files to include in the output
                included_files.push(entry_path.clone());
            }
        } else {
            // Always include directories in the tree view
            result.push_str(&format!(
                "{}{}{}",
                prefix,
                connector,
                entry.file_name().to_string_lossy()
            ));
            result.push_str("/\n");

            // Recursively process subdirectories
            generate_tree_recursive(
                &entry_path,
                result,
                &next_prefix,
                is_entry_last,
                extensions,
                max_size_kb,
                included_files
            )?;
        }
    }

    Ok(())
}

