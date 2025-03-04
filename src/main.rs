use anyhow::{Context, Result};
use arboard::Clipboard;
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Arg, Command};
use phf::phf_map;
use rayon::prelude::*;
use std::{fs, thread, time::Duration};
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

fn main() -> Result<()> {
    let matches = Command::new("codump")
        .about("dumps project files in an llm-friendly format")
        .arg(
            Arg::new("directory")
                .help("directory to scan (defaults to current)")
                .default_value(".")
                .index(1),
        )
        .arg(
            Arg::new("clipboard")
                .short('c')
                .long("clipboard")
                .help("copy output to clipboard instead of stdout")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("extensions")
                .short('e')
                .long("extensions")
                .help("file extensions to include (comma-separated)")
                .default_value("rs,py,js,ts,jsx,tsx,go,java,c,cpp,h,hpp,cs,rb,php,scala,kt,pl,sh,bash,zsh,fish,json,toml,yaml,yml,md,txt,csv,xml,sql,graphql,prisma,html,css,scss,sass,less"),
        )
        .arg(
            Arg::new("max-size")
                .short('s')
                .long("max-size")
                .help("max file size in kb to include")
                .default_value("100")
                .value_parser(clap::value_parser!(usize)),
        )
        .arg(
            Arg::new("exclude")
                .short('x')
                .long("exclude")
                .help("directories to exclude (comma-separated)")
                .default_value(".git,node_modules,target,dist,build,venv,__pycache__,.idea,.vscode,bin,obj,.mypy_cache,debug,.fingerprint,.cache"),
        )
        .arg(
            Arg::new("max-files")
                .long("max-files")
                .help("maximum number of files to include")
                .default_value("1000")
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
    let exclude_dirs = matches
        .get_one::<String>("exclude")
        .unwrap()
        .split(',')
        .map(|s| s.trim())
        .collect::<Vec<&str>>();
    let max_files = *matches.get_one::<usize>("max-files").unwrap();

    let output = generate_dump(
        directory,
        &extensions,
        max_size_kb,
        &exclude_dirs,
        max_files,
    )?;

    if to_clipboard {
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
            let full_path = base.join(relative_path);
            let content = fs::read_to_string(full_path.as_std_path())
                .with_context(|| format!("failed to read {}", full_path))?;
            let ext = relative_path.extension().unwrap_or("").to_lowercase();
            let lang = language_for_extension(&ext, &content);
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
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !exclude_dirs.iter().any(|d| name == *d)
        });

    for entry in walker {
        let entry = entry?;
        let entry_path = entry.path();
        let file_name = entry.file_name().to_string_lossy();
        let is_excluded = entry.depth() > 0 && exclude_dirs.iter().any(|d| file_name == *d);
        if is_excluded {
            continue;
        }
        if file_count >= max_files {
            break;
        }
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
                files.push(rel_path.clone());
                tree.push_str(&format!("{} [{}kb]\n", rel_path, size_kb));
            }
        } else if entry.file_type().is_dir() {
            let rel_path = Utf8Path::from_path(entry_path)
                .and_then(|p| p.strip_prefix(path).ok())
                .unwrap_or_else(|| Utf8Path::from_path(entry_path).unwrap())
                .to_owned();
            tree.push_str(&format!("{}/\n", rel_path));
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
    content
        .lines()
        .next()
        .map(|line| {
            if line.contains("python") {
                "python"
            } else if line.contains("ruby") {
                "ruby"
            } else if line.contains("node") {
                "javascript"
            } else {
                ""
            }
        })
        .unwrap_or("")
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
