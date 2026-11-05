mod output;

use std::ffi::OsString;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, anyhow};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Serialize, Serializer};
use serde_json::{Value, json};

use plamo_tr_local::engine::{InputError, Options, Term, Translator};
use plamo_tr_local::lang::{LANGUAGES, parse_language, resolve};
use plamo_tr_local::model::{self, ModelSize};

use output::OutputFormat;

const GLOBAL_OPTIONS_WITH_VALUE: &[&str] = &["--output"];

/// Translate text with PLaMo 3 Translate running on this machine.
#[derive(Parser)]
#[command(version, disable_help_subcommand = true, subcommand_required = true)]
struct Cli {
    /// Format of the output.
    #[arg(long, global = true, help_heading = "Global Options", value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// Print the parameters and the full error chain to stderr.
    #[arg(long, global = true, help_heading = "Global Options")]
    debug: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Translate text.
    #[command(subcommand_required = true)]
    Text {
        #[command(subcommand)]
        command: TextCommand,
    },
    /// Languages supported by PLaMo 3 Translate.
    #[command(subcommand_required = true)]
    Language {
        #[command(subcommand)]
        command: LanguageCommand,
    },
    /// PLaMo 3 Translate model files.
    #[command(subcommand_required = true)]
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
}

#[derive(Subcommand)]
enum TextCommand {
    /// Translate a text.
    ///
    /// Without --source-language-code the source language is detected
    /// automatically. Without --target-language-code, Japanese text is
    /// translated into English and anything else into Japanese. Either the
    /// source or the target must be ja or en.
    TranslateText(TranslateTextArgs),
}

#[derive(Subcommand)]
enum LanguageCommand {
    /// List the supported language codes.
    ListLanguages,
}

#[derive(Subcommand)]
enum ModelCommand {
    /// List the model sizes and whether each one is downloaded.
    ListModels,
    /// Download a model into the cache directory.
    DownloadModel(DownloadModelArgs),
}

#[derive(Args)]
struct TranslateTextArgs {
    /// Text to translate. Read from standard input when omitted.
    #[arg(long, value_parser = parse_value)]
    text: Option<String>,

    /// Source language code (detected when omitted).
    #[arg(long, value_parser = parse_language_code)]
    source_language_code: Option<LanguageCode>,

    /// Target language code (en for Japanese text, ja otherwise).
    #[arg(long, value_parser = parse_language_code)]
    target_language_code: Option<LanguageCode>,

    /// Writing style of the translation.
    #[arg(long, value_enum)]
    writing_style: Option<WritingStyle>,

    /// Text that helps the model read the input, such as the surrounding
    /// paragraphs or the kind of page.
    #[arg(long, value_parser = parse_value)]
    context: Option<String>,

    /// Translations of terms, as Source=<term>,Target=<translation> separated
    /// by spaces.
    #[arg(long, num_args = 1.., value_parser = parse_term)]
    glossary: Option<Vec<GlossaryTerm>>,

    /// Size of the model, downloaded into the cache on first use (2b when
    /// omitted).
    #[arg(long, value_enum)]
    model_size: Option<ModelSizeArg>,

    /// GGUF file of PLaMo 3 Translate to use instead of a cached model.
    /// Defaults to $PLAMO_TR_LOCAL_MODEL_FILE.
    #[arg(long)]
    model_file: Option<PathBuf>,
}

#[derive(Args)]
struct DownloadModelArgs {
    /// Size of the model to download (2b when omitted).
    #[arg(long, value_enum)]
    model_size: Option<ModelSizeArg>,
}

#[derive(Clone, Copy, Default, ValueEnum, Serialize)]
enum ModelSizeArg {
    #[value(name = "0.8b")]
    #[serde(rename = "0.8b")]
    Size0_8b,
    #[default]
    #[value(name = "2b")]
    #[serde(rename = "2b")]
    Size2b,
    #[value(name = "8b")]
    #[serde(rename = "8b")]
    Size8b,
    #[value(name = "31b")]
    #[serde(rename = "31b")]
    Size31b,
}

impl From<ModelSizeArg> for ModelSize {
    fn from(size: ModelSizeArg) -> Self {
        match size {
            ModelSizeArg::Size0_8b => ModelSize::Size0_8b,
            ModelSizeArg::Size2b => ModelSize::Size2b,
            ModelSizeArg::Size8b => ModelSize::Size8b,
            ModelSizeArg::Size31b => ModelSize::Size31b,
        }
    }
}

#[derive(Clone)]
struct GlossaryTerm {
    source: String,
    target: String,
}

fn parse_term(value: &str) -> Result<GlossaryTerm, String> {
    let expected = || format!("expected Source=<term>,Target=<translation>, got `{value}`");
    let mut source = None;
    let mut target = None;
    for pair in value.split(',') {
        match pair.split_once('=') {
            Some(("Source", term)) => source = Some(term.to_owned()),
            Some(("Target", term)) => target = Some(term.to_owned()),
            _ => return Err(expected()),
        }
    }
    match (source, target) {
        (Some(source), Some(target)) if !source.is_empty() && !target.is_empty() => {
            Ok(GlossaryTerm { source, target })
        }
        _ => Err(expected()),
    }
}

#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
enum WritingStyle {
    Plain,
    Polite,
}

impl WritingStyle {
    fn as_str(self) -> &'static str {
        match self {
            WritingStyle::Plain => "plain",
            WritingStyle::Polite => "polite",
        }
    }
}

#[derive(Clone, Copy)]
struct LanguageCode(&'static str);

impl Serialize for LanguageCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.0)
    }
}

fn parse_language_code(value: &str) -> Result<LanguageCode, String> {
    parse_language(value).map(LanguageCode)
}

fn parse_value(value: &str) -> Result<String, String> {
    match value.strip_prefix("file://") {
        Some(path) => std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}")),
        None => Ok(value.to_owned()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct TranslateTextOutput {
    translated_text: String,
    source_language_code: Option<&'static str>,
    target_language_code: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ListLanguagesOutput {
    languages: Vec<LanguageOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct LanguageOutput {
    language_code: &'static str,
    language_name: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ListModelsOutput {
    models: Vec<ModelOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ModelOutput {
    model_size: &'static str,
    url: String,
    path: PathBuf,
    downloaded: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct DownloadModelOutput {
    model_size: &'static str,
    path: PathBuf,
}

enum Failure {
    Usage(anyhow::Error),
    Target(anyhow::Error),
    Other(anyhow::Error),
}

impl Failure {
    fn exit_code(&self) -> u8 {
        match self {
            Failure::Usage(_) => 252,
            Failure::Target(_) => 254,
            Failure::Other(_) => 255,
        }
    }

    fn error(&self) -> &anyhow::Error {
        match self {
            Failure::Usage(error) | Failure::Target(error) | Failure::Other(error) => error,
        }
    }
}

impl From<anyhow::Error> for Failure {
    fn from(error: anyhow::Error) -> Self {
        if error.downcast_ref::<InputError>().is_some() {
            Failure::Target(error)
        } else {
            Failure::Other(error)
        }
    }
}

fn usage(message: impl std::fmt::Display) -> Failure {
    Failure::Usage(anyhow!("{message}"))
}

/// Turn a trailing `help` word into `--help`, so that `help` works at every
/// level of the command tree.
fn rewrite_help(mut args: Vec<OsString>) -> Vec<OsString> {
    let mut index = 1;
    while index < args.len() {
        let arg = args[index].to_string_lossy();
        if GLOBAL_OPTIONS_WITH_VALUE.contains(&arg.as_ref()) {
            index += 2;
            continue;
        }
        if arg == "help" && index == args.len() - 1 {
            args[index] = "--help".into();
            break;
        }
        let is_global = arg == "--debug"
            || GLOBAL_OPTIONS_WITH_VALUE
                .iter()
                .any(|option| arg.starts_with(&format!("{option}=")));
        if arg.starts_with('-') && !is_global {
            break;
        }
        index += 1;
    }
    args
}

fn model_file(file: Option<PathBuf>, size: Option<ModelSizeArg>) -> anyhow::Result<PathBuf> {
    match file.or_else(|| std::env::var_os("PLAMO_TR_LOCAL_MODEL_FILE").map(PathBuf::from)) {
        Some(path) => Ok(path),
        None => model::ensure(size.unwrap_or_default().into()),
    }
}

fn translate_text(args: TranslateTextArgs, debug: bool) -> Result<Value, Failure> {
    if debug {
        eprintln!(
            "parameters: {}",
            serde_json::to_string(&json!({
                "SourceLanguageCode": args.source_language_code,
                "TargetLanguageCode": args.target_language_code,
                "WritingStyle": args.writing_style,
                "ModelSize": args.model_size,
                "ModelFile": args.model_file,
            }))
            .unwrap_or_default()
        );
    }
    let text = match args.text {
        Some(text) => text,
        None if std::io::stdin().is_terminal() => {
            return Err(usage(
                "the required parameter --text was not provided, and standard input is a terminal",
            ));
        }
        None => {
            std::io::read_to_string(std::io::stdin()).context("failed to read standard input")?
        }
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(usage("the text to translate is empty"));
    }
    let direction = resolve(
        args.source_language_code.map(|code| code.0),
        args.target_language_code.map(|code| code.0),
        text,
    )
    .map_err(Failure::Target)?;
    if args.writing_style.is_some() && direction.target != "ja" {
        return Err(usage("--writing-style needs --target-language-code ja"));
    }
    let path = model_file(args.model_file, args.model_size)?;
    let translator = Translator::load(&path, debug)?;
    let glossary: Vec<Term> = args
        .glossary
        .unwrap_or_default()
        .into_iter()
        .map(|term| Term {
            source: term.source,
            target: term.target,
        })
        .collect();
    let options = Options {
        direction: &direction,
        writing_style: args.writing_style.map(WritingStyle::as_str),
        context: args.context.as_deref(),
        glossary: &glossary,
    };
    let translated_text = translator.translate(text, &options)?;
    to_value(TranslateTextOutput {
        translated_text,
        source_language_code: direction.source,
        target_language_code: direction.target,
    })
}

fn list_languages() -> ListLanguagesOutput {
    ListLanguagesOutput {
        languages: LANGUAGES
            .iter()
            .map(|language| LanguageOutput {
                language_code: language.code,
                language_name: language.name,
            })
            .collect(),
    }
}

fn list_models() -> anyhow::Result<ListModelsOutput> {
    let mut models = Vec::new();
    for size in ModelSize::ALL {
        let path = model::cached_path(size)?;
        models.push(ModelOutput {
            model_size: size.as_str(),
            url: size.url(),
            downloaded: path.exists(),
            path,
        });
    }
    Ok(ListModelsOutput { models })
}

fn download_model(args: DownloadModelArgs) -> anyhow::Result<DownloadModelOutput> {
    let size: ModelSize = args.model_size.unwrap_or_default().into();
    Ok(DownloadModelOutput {
        model_size: size.as_str(),
        path: model::ensure(size)?,
    })
}

fn to_value(output: impl Serialize) -> Result<Value, Failure> {
    serde_json::to_value(output).map_err(|error| Failure::Other(error.into()))
}

fn run(command: Command, debug: bool) -> Result<Value, Failure> {
    match command {
        Command::Text {
            command: TextCommand::TranslateText(args),
        } => translate_text(args, debug),
        Command::Language {
            command: LanguageCommand::ListLanguages,
        } => to_value(list_languages()),
        Command::Model {
            command: ModelCommand::ListModels,
        } => to_value(list_models()?),
        Command::Model {
            command: ModelCommand::DownloadModel(args),
        } => to_value(download_model(args)?),
    }
}

fn main() -> ExitCode {
    let args = rewrite_help(std::env::args_os().collect());
    let cli = match Cli::try_parse_from(args) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(if error.use_stderr() { 252 } else { 0 });
        }
    };
    let format = cli.output;
    let debug = cli.debug;
    let result = run(cli.command, debug).map(|value| {
        println!("{}", output::render(&value, format));
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            if debug {
                eprintln!("error: {:?}", failure.error());
            } else {
                eprintln!("error: {:#}", failure.error());
            }
            ExitCode::from(failure.exit_code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn parse(words: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(rewrite_help(args(words)))
    }

    #[test]
    fn help_works_at_every_level() {
        for words in [
            &["plamo-tr-local", "help"][..],
            &["plamo-tr-local", "text", "help"],
            &[
                "plamo-tr-local",
                "--output",
                "text",
                "text",
                "translate-text",
                "help",
            ],
        ] {
            let error = parse(words).err().expect("help is not a command");
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        }
    }

    #[test]
    fn help_as_a_parameter_value_is_kept() {
        let cli = parse(&["plamo-tr-local", "text", "translate-text", "--text", "help"]).unwrap();
        let Command::Text {
            command: TextCommand::TranslateText(args),
        } = cli.command
        else {
            panic!("parsed another command");
        };
        assert_eq!(args.text.as_deref(), Some("help"));
    }

    #[test]
    fn rejects_positional_arguments() {
        assert!(parse(&["plamo-tr-local", "text", "translate-text", "こんにちは"]).is_err());
    }

    #[test]
    fn reads_values_from_files() {
        let path = std::env::temp_dir().join("plamo-tr-local-parse-value-test.txt");
        std::fs::write(&path, "こんにちは").unwrap();
        assert_eq!(
            parse_value(&format!("file://{}", path.display())),
            Ok("こんにちは".to_owned())
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(parse_value("Hello"), Ok("Hello".to_owned()));
    }

    #[test]
    fn input_errors_exit_with_254() {
        let failure = Failure::from(anyhow::Error::new(InputError("too long".into())));
        assert_eq!(failure.exit_code(), 254);
        assert_eq!(Failure::from(anyhow!("network")).exit_code(), 255);
    }

    #[test]
    fn languages_are_listed_with_pascal_case_keys() {
        let value = to_value(list_languages()).ok().unwrap();
        assert_eq!(
            value["Languages"][0],
            json!({"LanguageCode": "ja", "LanguageName": "Japanese"})
        );
    }

    #[test]
    fn glossary_takes_shorthand() {
        let cli = parse(&[
            "plamo-tr-local",
            "text",
            "translate-text",
            "--text",
            "Filter by owner.",
            "--glossary",
            "Source=owner,Target=担当者",
            "Source=task,Target=タスク",
        ])
        .unwrap();
        let Command::Text {
            command: TextCommand::TranslateText(args),
        } = cli.command
        else {
            panic!("parsed another command");
        };
        let glossary = args.glossary.unwrap();
        assert_eq!(glossary.len(), 2);
        assert_eq!(glossary[0].target, "担当者");
        assert!(parse_term("Source=owner").is_err());
    }
}
