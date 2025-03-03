# dump

dump is a simple utility that outputs your project's files in an llm-friendly format.
it generates a tree view of your project structure and then includes the file contents
with syntax highlighting.

## features

- generates a tree view of your project directory
- dumps file contents with language detection
- supports excluding specified directories
- optionally copies output to clipboard
- uses rayon for fast, parallel file processing

## usage

run dump from the terminal:

```bash
cargo run -- [directory]
```

if no directory is provided, dump uses the current directory.

command line options:

- `-c, --clipboard`: copy output to clipboard instead of stdout
- `-e, --extensions`: comma-separated file extensions to include
- `-s, --max-size`: maximum file size in kb to include (default: 100)
- `-x, --exclude`: comma-separated directories to exclude
- `--max-files`: maximum number of files to include (default: 1000)

## installation

1. clone the repo.
2. run `cargo build --release`
3. run the binary from `./target/release/dump`

## note

dump was built with performance in mind and leverages rayon for parallel file scanning.
