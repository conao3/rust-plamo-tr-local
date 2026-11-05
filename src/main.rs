mod output;

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::anyhow;
use clap::{ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value, json};

use plamo_tr_local::engine::{InputError, Options, Term, Translator};
use plamo_tr_local::lang::{LANGUAGES, parse_language, resolve};
use plamo_tr_local::model::{self, ModelSize};

use output::OutputFormat;

const GLOBAL_OPTIONS_WITH_VALUE: &[&str] = &["--output", "--query"];
const INPUT_OPTIONS: &[&str] = &["cli-input-json", "generate-cli-skeleton", "help"];

/// Translate text with PLaMo 3 Translate running on this machine.
#[derive(Parser)]
#[command(version, disable_help_subcommand = true, subcommand_required = true)]
struct Cli {
    /// Format of the output.
    #[arg(long, global = true, help_heading = "Global Options", value_enum, default_value_t = OutputFormat::Json)]
    output: OutputFormat,

    /// JMESPath expression that selects part of the output.
    #[arg(long, global = true, help_heading = "Global Options")]
    query: Option<String>,

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
    ListLanguages(ListLanguagesArgs),
}

#[derive(Subcommand)]
enum ModelCommand {
    /// List the model sizes and whether each one is downloaded.
    ListModels(ListModelsArgs),
    /// Download a model into the cache directory.
    DownloadModel(DownloadModelArgs),
}

#[derive(Args, Default)]
struct InputArgs {
    /// Read the parameters from JSON (PascalCase keys). Parameters given on
    /// the command line take precedence.
    #[arg(long, value_parser = parse_value)]
    cli_input_json: Option<String>,

    /// Print a JSON skeleton of the input or the output and exit.
    #[arg(long, value_enum, num_args = 0..=1, default_missing_value = "input")]
    generate_cli_skeleton: Option<Skeleton>,
}

#[derive(Clone, Copy, ValueEnum)]
enum Skeleton {
    Input,
    Output,
}

#[derive(Args, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct TranslateTextArgs {
    /// Text to translate. Use file:///dev/stdin to read standard input.
    #[arg(long, value_parser = parse_value)]
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,

    /// Source language code (detected when omitted).
    #[arg(long, value_parser = parse_language_code)]
    #[serde(skip_serializing_if = "Option::is_none")]
    source_language_code: Option<LanguageCode>,

    /// Target language code (en for Japanese text, ja otherwise).
    #[arg(long, value_parser = parse_language_code)]
    #[serde(skip_serializing_if = "Option::is_none")]
    target_language_code: Option<LanguageCode>,

    /// Writing style of the translation.
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    writing_style: Option<WritingStyle>,

    /// Text that helps the model read the input, such as the surrounding
    /// paragraphs or the kind of page.
    #[arg(long, value_parser = parse_value)]
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,

    /// Translations of terms, as Source=<term>,Target=<translation> separated
    /// by spaces.
    #[arg(long, num_args = 1.., value_parser = parse_term)]
    #[serde(skip_serializing_if = "Option::is_none")]
    glossary: Option<Vec<GlossaryTerm>>,

    /// Size of the model, downloaded into the cache on first use (2b when
    /// omitted).
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    model_size: Option<ModelSizeArg>,

    /// GGUF file of PLaMo 3 Translate to use instead of a cached model.
    /// Defaults to $PLAMO_TR_LOCAL_MODEL_FILE.
    #[arg(long)]
    #[serde(skip_serializing_if = "Option::is_none")]
    model_file: Option<PathBuf>,

    #[command(flatten)]
    #[serde(skip)]
    input: InputArgs,
}

#[derive(Args, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct ListLanguagesArgs {
    #[command(flatten)]
    #[serde(skip)]
    input: InputArgs,
}

#[derive(Args, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct ListModelsArgs {
    #[command(flatten)]
    #[serde(skip)]
    input: InputArgs,
}

#[derive(Args, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
struct DownloadModelArgs {
    /// Size of the model to download (2b when omitted).
    #[arg(long, value_enum)]
    #[serde(skip_serializing_if = "Option::is_none")]
    model_size: Option<ModelSizeArg>,

    #[command(flatten)]
    #[serde(skip)]
    input: InputArgs,
}

#[derive(Clone, Copy, Default, ValueEnum, Serialize, Deserialize)]
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
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

#[derive(Clone, Copy, ValueEnum, Serialize, Deserialize)]
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

impl<'de> Deserialize<'de> for LanguageCode {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        parse_language_code(&value).map_err(serde::de::Error::custom)
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

fn subcommand_path(matches: &clap::ArgMatches) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = matches;
    while let Some((name, sub)) = current.subcommand() {
        path.push(name.to_owned());
        current = sub;
    }
    path
}

fn pascal_case(kebab: &str) -> String {
    kebab
        .split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// The input skeleton of a subcommand, derived from its clap definition.
fn input_skeleton(path: &[String]) -> Value {
    let mut command = Cli::command();
    for name in path {
        command = command
            .find_subcommand(name)
            .expect("subcommand path comes from the parsed arguments")
            .clone();
    }
    let mut skeleton = Map::new();
    for arg in command.get_arguments() {
        let Some(long) = arg.get_long() else { continue };
        if INPUT_OPTIONS.contains(&long) || arg.is_global_set() {
            continue;
        }
        let value = match arg.get_action() {
            ArgAction::SetTrue | ArgAction::SetFalse => json!(false),
            _ if long == "glossary" => json!([{"Source": "", "Target": ""}]),
            _ => match arg.get_possible_values().first() {
                Some(possible) => json!(possible.get_name()),
                None => json!(""),
            },
        };
        skeleton.insert(pascal_case(long), value);
    }
    Value::Object(skeleton)
}

/// Overlay the parameters given on the command line onto `--cli-input-json`.
fn merge_input<T: Serialize + DeserializeOwned>(args: &T, input: &InputArgs) -> Result<T, Failure> {
    let given = serde_json::to_value(args).map_err(|error| Failure::Other(error.into()))?;
    let Some(json) = &input.cli_input_json else {
        return serde_json::from_value(given).map_err(usage);
    };
    let mut merged: Value = serde_json::from_str(json)
        .map_err(|error| usage(format!("--cli-input-json is not valid JSON: {error}")))?;
    let (Value::Object(merged_map), Value::Object(given_map)) = (&mut merged, given) else {
        return Err(usage("--cli-input-json must be a JSON object"));
    };
    merged_map.extend(given_map);
    serde_json::from_value(merged).map_err(|error| usage(format!("--cli-input-json: {error}")))
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
    let Some(text) = args.text else {
        return Err(usage("the required parameter --text was not provided"));
    };
    let text = text.trim();
    if text.is_empty() {
        return Err(usage("--text is empty"));
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

/// Run the subcommand. `None` means a skeleton was printed instead.
fn run(cli: Cli, path: &[String]) -> Result<Option<Value>, Failure> {
    let (input, output_skeleton) = match &cli.command {
        Command::Text {
            command: TextCommand::TranslateText(args),
        } => (
            &args.input,
            to_value(TranslateTextOutput {
                translated_text: String::new(),
                source_language_code: Some(""),
                target_language_code: "",
            })?,
        ),
        Command::Language {
            command: LanguageCommand::ListLanguages(args),
        } => (
            &args.input,
            to_value(ListLanguagesOutput {
                languages: vec![LanguageOutput {
                    language_code: "",
                    language_name: "",
                }],
            })?,
        ),
        Command::Model {
            command: ModelCommand::ListModels(args),
        } => (
            &args.input,
            to_value(ListModelsOutput {
                models: vec![ModelOutput {
                    model_size: "",
                    url: String::new(),
                    path: PathBuf::new(),
                    downloaded: false,
                }],
            })?,
        ),
        Command::Model {
            command: ModelCommand::DownloadModel(args),
        } => (
            &args.input,
            to_value(DownloadModelOutput {
                model_size: "",
                path: PathBuf::new(),
            })?,
        ),
    };
    match input.generate_cli_skeleton {
        Some(Skeleton::Input) => {
            println!("{}", output::json(&input_skeleton(path)));
            return Ok(None);
        }
        Some(Skeleton::Output) => {
            println!("{}", output::json(&output_skeleton));
            return Ok(None);
        }
        None => {}
    }
    let value = match cli.command {
        Command::Text {
            command: TextCommand::TranslateText(args),
        } => {
            let args = merge_input(&args, &args.input)?;
            translate_text(args, cli.debug)?
        }
        Command::Language {
            command: LanguageCommand::ListLanguages(args),
        } => {
            merge_input(&args, &args.input)?;
            to_value(list_languages())?
        }
        Command::Model {
            command: ModelCommand::ListModels(args),
        } => {
            merge_input(&args, &args.input)?;
            to_value(list_models()?)?
        }
        Command::Model {
            command: ModelCommand::DownloadModel(args),
        } => {
            let args = merge_input(&args, &args.input)?;
            to_value(download_model(args)?)?
        }
    };
    Ok(Some(value))
}

fn main() -> ExitCode {
    let args = rewrite_help(std::env::args_os().collect());
    let matches = match Cli::command().try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(if error.use_stderr() { 252 } else { 0 });
        }
    };
    let path = subcommand_path(&matches);
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(252);
        }
    };
    let format = cli.output;
    let debug = cli.debug;
    let query = match cli.query.as_deref().map(jmespath::compile).transpose() {
        Ok(query) => query,
        Err(error) => {
            eprintln!("error: invalid --query: {error}");
            return ExitCode::from(252);
        }
    };
    let result = run(cli, &path).and_then(|value| {
        let Some(value) = value else { return Ok(()) };
        let value = match &query {
            Some(query) => {
                let found = query
                    .search(&value)
                    .map_err(|error| usage(format!("--query: {error}")))?;
                serde_json::to_value(&*found).map_err(|error| Failure::Other(error.into()))?
            }
            None => value,
        };
        println!("{}", output::render(&value, format));
        Ok(())
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
    fn input_skeleton_is_accepted_as_cli_input_json() {
        let path = ["text".to_owned(), "translate-text".to_owned()];
        let skeleton = input_skeleton(&path);
        assert_eq!(
            skeleton,
            json!({
                "Text": "",
                "SourceLanguageCode": "",
                "TargetLanguageCode": "",
                "WritingStyle": "plain",
                "Context": "",
                "Glossary": [{"Source": "", "Target": ""}],
                "ModelSize": "0.8b",
                "ModelFile": "",
            })
        );
        let mut keys: Map<String, Value> = skeleton.as_object().unwrap().clone();
        keys.insert("SourceLanguageCode".into(), json!("ja"));
        keys.insert("TargetLanguageCode".into(), json!("en"));
        let input = InputArgs {
            cli_input_json: Some(Value::Object(keys).to_string()),
            generate_cli_skeleton: None,
        };
        assert!(merge_input(&TranslateTextArgs::default(), &input).is_ok());
    }

    #[test]
    fn command_line_overrides_cli_input_json() {
        let cli = parse(&[
            "plamo-tr-local",
            "text",
            "translate-text",
            "--target-language-code",
            "ko",
            "--cli-input-json",
            r#"{"Text":"Hello","TargetLanguageCode":"fr"}"#,
        ])
        .unwrap();
        let Command::Text {
            command: TextCommand::TranslateText(args),
        } = cli.command
        else {
            panic!("parsed another command");
        };
        let merged = merge_input(&args, &args.input).ok().unwrap();
        assert_eq!(merged.text.as_deref(), Some("Hello"));
        assert_eq!(merged.target_language_code.map(|code| code.0), Some("ko"));
    }

    #[test]
    fn cli_input_json_rejects_unknown_keys_and_languages() {
        for json in [r#"{"Txt":"Hello"}"#, r#"{"TargetLanguageCode":"xx"}"#] {
            let input = InputArgs {
                cli_input_json: Some(json.to_owned()),
                generate_cli_skeleton: None,
            };
            let failure = merge_input(&TranslateTextArgs::default(), &input)
                .err()
                .expect("invalid input");
            assert_eq!(failure.exit_code(), 252);
        }
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
    fn glossary_takes_shorthand_and_json() {
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
        let input = InputArgs {
            cli_input_json: Some(
                r#"{"Glossary":[{"Source":"owner","Target":"担当者"}],"ModelSize":"8b"}"#
                    .to_owned(),
            ),
            generate_cli_skeleton: None,
        };
        assert!(merge_input(&TranslateTextArgs::default(), &input).is_ok());
    }
}
