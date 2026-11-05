<p align="center">
  <img src="docs/assets/banner.png" alt="plamo-tr-local: Translate text from the command line with PLaMo 3 Translate running on your machine" width="880">
</p>

<p align="center"><strong>Translate text from the command line with PLaMo 3 Translate running on your machine</strong></p>

<p align="center">
  <a href="https://github.com/conao3/rust-plamo-tr-local/actions/workflows/ci.yml"><img src="https://github.com/conao3/rust-plamo-tr-local/actions/workflows/ci.yml/badge.svg?branch=master" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/conao3/rust-plamo-tr-local" alt="License: Apache-2.0"></a>
  <a href="https://github.com/conao3/rust-plamo-tr-local/stargazers"><img src="https://img.shields.io/github/stars/conao3/rust-plamo-tr-local?style=flat" alt="GitHub stars"></a>
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#usage">Usage</a> ·
  <a href="#commands">Commands</a> ·
  <a href="#models">Models</a> ·
  <a href="#license">License</a> ·
  <a href="#development">Development</a>
</p>

# plamo-tr-local

`plamo-tr-local` translates text in your terminal with [PLaMo 3 Translate](https://huggingface.co/pfnet/plamo-3-translate-2b), the translation model by Preferred Networks. It runs the model on your own machine through [llama.cpp](https://github.com/ggml-org/llama.cpp).

- **Your text stays on your machine.** You need no account and no API key.
- **It speaks the [plamo-tr](https://github.com/conao3/rust-plamo-tr) command line.** You get `<command> <subcommand>` with named parameters, JSON output, `--query`, `--output text`, and `--cli-input-json`. A `plamo-tr text translate-text` call without `--auth-file` runs unchanged under `plamo-tr-local`.
- **It fits a shell pipeline.** Pipe text in and pick the translation out with a JMESPath query.
- **You pick the model size.** Choose from `0.8b` to `31b`. On the first run, `plamo-tr-local` downloads the model from Hugging Face into your cache directory.

Built with PLaMo.

> [!NOTE]
> `plamo-tr-local` is an unofficial tool and has no affiliation with Preferred Networks. Preferred Networks distributes the model under the [PLaMo Community License](https://plamo.preferredai.jp/info/plamo-community-license-en); see [License](#license).

## Install

Run it with Nix, or install it into your profile:

```console
$ nix run github:conao3/rust-plamo-tr-local -- text translate-text --text 今日はいい天気ですね。
$ nix profile install github:conao3/rust-plamo-tr-local
```

Or build it with Cargo. Cargo compiles llama.cpp along the way, so you need CMake, a C++ compiler, and libclang:

```console
$ cargo install --git https://github.com/conao3/rust-plamo-tr-local
```

## Usage

Translate Japanese into English. `plamo-tr-local` prints JSON unless you ask for another format:

```console
$ plamo-tr-local text translate-text --text 今日はいい天気ですね。
{
  "TranslatedText": "The weather is nice today, isn't it?",
  "SourceLanguageCode": null,
  "TargetLanguageCode": "en"
}
```

Read the text from standard input, and print only the translation with `--query` and `--output text`:

```console
$ echo "Since it's raining today, I think I'll relax at home and read a book." \
    | plamo-tr-local text translate-text --text file:///dev/stdin --query TranslatedText --output text
今日は雨だから、家でゆっくり本を読んでいようと思う。
```

Fix the translation of a term with `--glossary`, and set the writing style of Japanese with `--writing-style`:

```console
$ plamo-tr-local text translate-text --text "Filter tasks by owner." \
    --glossary Source=owner,Target=担当者 --writing-style polite --query TranslatedText --output text
担当者でタスクを絞り込みます。
```

Trade speed for accuracy with `--model-size`. Smaller models run faster, and larger ones translate better:

```console
$ plamo-tr-local text translate-text --model-size 0.8b --text 今日はいい天気ですね。
```

## Commands

The command line follows the grammar of the AWS CLI: `plamo-tr-local [global options] <command> <subcommand> [--parameter value ...]`. You name each parameter, and `plamo-tr-local` takes no positional arguments.

| Command | Subcommand | Output |
|---|---|---|
| `text` | `translate-text` | `TranslatedText`, `SourceLanguageCode` (`null` when the model detects it), `TargetLanguageCode` |
| `language` | `list-languages` | `Languages`, a list of `LanguageCode` and `LanguageName` |
| `model` | `list-models` | `Models`, a list of `ModelSize`, `Url`, `Path`, and `Downloaded` |
| `model` | `download-model` | `ModelSize` and `Path` of the downloaded model |

### `text translate-text`

| Parameter | Meaning |
|---|---|
| `--text <TEXT>` | Text to translate. Required. `file:///dev/stdin` reads standard input. |
| `--source-language-code <LANG>` | Source language. When omitted, the model or `plamo-tr-local` detects it (see below). |
| `--target-language-code <LANG>` | Target language. When omitted, `plamo-tr-local` translates Japanese text into English and anything else into Japanese. |
| `--writing-style <plain\|polite>` | Writing style of a Japanese translation. Needs the target `ja`. |
| `--context <TEXT>` | Text that helps the model read the input, such as the surrounding paragraphs or the kind of page. |
| `--glossary <Source=<term>,Target=<translation> ...>` | Translations of terms, separated by spaces. |
| `--model-size <0.8b\|2b\|8b\|31b>` | Size of the model. Defaults to `2b`. |
| `--model-file <PATH>` | A GGUF file of PLaMo 3 Translate to use instead of a cached model. Defaults to `PLAMO_TR_LOCAL_MODEL_FILE`. |

Either the source or the target must be `ja` or `en`. If you omit the source and the target is `ja` or `en`, the model detects the source language itself. For other targets, `plamo-tr-local` detects the source language with [whatlang](https://github.com/greyblake/whatlang-rs) and passes it to the model.

The model reads up to 32,768 tokens of input and output together. For a long text, `plamo-tr-local` translates a few paragraphs at a time and joins the translations with the original paragraph and line breaks.

### `model list-models` and `model download-model`

`model download-model --model-size <SIZE>` downloads a model into the cache before your first translation. `model list-models` shows where each model lives and whether you have downloaded it.

### Input from JSON and files

Every subcommand also takes:

- `--cli-input-json <JSON>`: all parameters as one JSON object whose keys are the parameter names in PascalCase (`Text`, `SourceLanguageCode`, `Glossary`, ...). Parameters given on the command line take precedence.
- `--generate-cli-skeleton [input|output]`: print a JSON skeleton of the input (default) or the output and exit.

A value that starts with `file://` names a file to read, for example `--text file://article.txt` or `--cli-input-json file://input.json`.

### Global options

| Option | Meaning |
|---|---|
| `--output <json\|text\|table>` | Output format. Defaults to `json`. `text` prints the values of an object tab-separated on one line and each list item on its own line. |
| `--query <JMESPath>` | Select part of the output with a [JMESPath](https://jmespath.org/) expression, for example `TranslatedText`. |
| `--debug` | Print the parameters, the llama.cpp log, the model load time, the token counts and timings (to the first token and of the whole output), and the full error chain to stderr. |
| `--version` | Print the version. |

`help` prints the help of each level: `plamo-tr-local help`, `plamo-tr-local text help`, `plamo-tr-local text translate-text help`.

### Environment variables

| Variable | Meaning |
|---|---|
| `PLAMO_TR_LOCAL_MODEL_FILE` | GGUF file to use when `--model-file` is not given. |
| `PLAMO_TR_LOCAL_CACHE_DIR` | Directory for downloaded models. Defaults to `plamo-tr-local` in the user cache directory (`~/.cache/plamo-tr-local` on Linux). |
| `PLAMO_TR_LOCAL_THREADS` | Number of CPU threads. Defaults to half the logical CPUs. |

### Exit codes

| Exit code | Meaning |
|---|---|
| 0 | Success. |
| 252 | The command line or a parameter is invalid (unknown command or parameter, unsupported language code, missing or empty `--text`, `--writing-style` without the target `ja`, invalid `--cli-input-json` or `--query`). |
| 254 | The model cannot take the input: the language pair is not supported, the input contains the model's reserved tokens (`<\|plamo:`), or a line is too long. |
| 255 | Any other failure, such as a failed download or an unreadable model file. |

## Models

`plamo-tr-local` uses the Q4_K_M GGUF files of [pfnet/plamo-3-translate-*-GGUF](https://huggingface.co/pfnet/plamo-3-translate-2b-GGUF):

| `--model-size` | File size |
|---|---|
| `0.8b` | 573 MB |
| `2b` | 1.81 GB |
| `8b` | 5.28 GB |
| `31b` | about 19.5 GB |

On an Intel Core Ultra 5 225U (CPU only), translating one English sentence of about 20 words into Japanese takes about 7 seconds with `0.8b` and 17 seconds with `2b`, including loading the model.

The x86_64 Linux build targets `x86-64-v3` (AVX2, from Intel Haswell and AMD Excavator on), so llama.cpp uses the vector instructions.

## License

You can use `plamo-tr-local` under the [Apache License 2.0](LICENSE).

Preferred Networks licenses the PLaMo 3 Translate models that `plamo-tr-local` downloads under the [PLaMo Community License](https://plamo.preferredai.jp/info/plamo-community-license-en). Private and non-commercial use is free. Organizations with annual revenue of 1 billion yen or less can use the model commercially for free after they register on the [PLaMo commercial use form](https://forms.gle/mTL8tBLrMYXKNZD56). When you publish translations from the model, state that they are outputs of PLaMo. Read the license before you use the model.

## Development

```console
$ nix develop
$ make check
```

`make check` runs `nix fmt -- --ci`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. The tests do not load a model.
