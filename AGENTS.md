# AGENTS.md

`plamo-tr-local` is a Rust CLI that translates text with the PLaMo 3 Translate GGUF models running locally through llama.cpp (the `llama-cpp-2` crate).

## Layout

- `src/lib.rs`: the library crate (`plamo_tr_local::engine`, `lang`, and `model`).
- `src/main.rs`: the command table (clap derive: `text translate-text`, `language list-languages`, `model list-models`, `model download-model`, global options), `file://` values, `--cli-input-json` and `--generate-cli-skeleton` derived from that table, `--query`, and exit codes.
- `src/output.rs`: the `json`, `text`, and `table` output formats.
- `src/lang.rs`: supported languages, local detection with whatlang, and the rules that pick the source and target.
- `src/engine.rs`: the PFML3 prompt (context, glossary, input and output language, `form`), splitting long text into paragraphs, and greedy generation with llama.cpp.
- `src/model.rs`: model sizes, the Hugging Face URLs of the Q4_K_M GGUF files, and the download into the cache directory.

## Conventions

- The prompt follows the `chat_template.jinja` that ships with the GGUF repositories: language codes are lowercase (`zh-hans`), and `form` goes only with the target `ja`.
- Either the source or the target must be `ja` or `en`.
- The command line follows the AWS CLI grammar: `<command> <subcommand>` with named `--kebab-case` parameters only, PascalCase keys in the output and in `--cli-input-json`, and exit codes 0 / 252 (command line or parameter) / 254 (input the model cannot take) / 255 (other). A new parameter is a field of the subcommand's `Args` struct; its help, skeleton entry, and `--cli-input-json` key follow from that field.
- `llama-cpp-2` is a git dependency pinned to a revision whose llama.cpp reads the `plamo3` tokenizer. After changing the revision, update `outputHashes` in `flake.nix`.
- `.cargo/config.toml` builds x86_64 Linux for `x86-64-v3`; llama.cpp picks its CPU kernels from the Rust target features.
- Commit messages, comments, and docs are in English.

## Verification

Run everything inside the devShell:

```console
$ nix develop --command make check
$ nix build
```

The tests do not load a model. To try a real translation, run `cargo run --release -- text translate-text --model-size 0.8b --text <text>`; set `PLAMO_TR_LOCAL_CACHE_DIR` to keep the downloaded model out of your user cache.
