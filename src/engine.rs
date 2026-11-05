use std::num::NonZeroU32;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{LogOptions, send_logs_to_tracing};

use crate::lang::Direction;

const RESERVED: &str = "<|plamo:";
const STOP_TOKENS: &[&str] = &["<|plamo:tag|>", "<|plamo:bos|>"];
const CHUNK_TOKENS: usize = 4096;

/// The model cannot translate the input (too long, or it contains PFML tokens).
#[derive(Debug)]
pub struct InputError(pub String);

impl std::fmt::Display for InputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InputError {}

pub struct Term {
    pub source: String,
    pub target: String,
}

pub struct Options<'a> {
    pub direction: &'a Direction,
    pub writing_style: Option<&'a str>,
    pub context: Option<&'a str>,
    pub glossary: &'a [Term],
}

/// Build the PFML3 prompt of one translation, without the leading BOS token.
pub fn prompt(text: &str, options: &Options) -> String {
    let mut prompt = String::from("<|plamo:tag|>dataset<|plamo:msg|>translation");
    if let Some(context) = options.context.filter(|context| !context.is_empty()) {
        prompt.push_str("<|plamo:tag|>context<|plamo:msg|>");
        prompt.push_str(context);
    }
    if !options.glossary.is_empty() {
        prompt.push_str("<|plamo:tag|>glossary");
        for term in options.glossary {
            prompt.push_str("<|plamo:index|>");
            prompt.push_str(&term.source);
            prompt.push_str("<|plamo:item|>");
            prompt.push_str(&term.target);
        }
    }
    prompt.push_str("<|plamo:tag|>input");
    if let Some(source) = options.direction.source {
        prompt.push_str("<|plamo:key|>lang<|plamo:val|>");
        prompt.push_str(&source.to_ascii_lowercase());
    }
    prompt.push_str("<|plamo:msg|>");
    prompt.push_str(text);
    prompt.push_str("<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>");
    prompt.push_str(&options.direction.target.to_ascii_lowercase());
    if let Some(form) = options.writing_style {
        prompt.push_str("<|plamo:key|>form<|plamo:val|>");
        prompt.push_str(form);
    }
    prompt.push_str("<|plamo:msg|>");
    prompt
}

fn check_input(text: &str, options: &Options) -> Result<()> {
    let fields = [Some(text), options.context].into_iter().flatten().chain(
        options
            .glossary
            .iter()
            .flat_map(|term| [term.source.as_str(), term.target.as_str()]),
    );
    for field in fields {
        if field.contains(RESERVED) {
            return Err(InputError(format!("the input must not contain `{RESERVED}`")).into());
        }
    }
    if options.writing_style.is_some() && options.direction.target != "ja" {
        return Err(InputError("--writing-style needs the target language ja".into()).into());
    }
    Ok(())
}

/// Group `units` joined by `separator` into pieces whose token counts stay
/// under `limit`. A unit over the limit is returned alone.
fn group<'a>(
    units: impl Iterator<Item = &'a str>,
    separator: &str,
    limit: usize,
    count: &impl Fn(&str) -> usize,
) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for unit in units {
        let candidate = if current.is_empty() {
            unit.to_owned()
        } else {
            format!("{current}{separator}{unit}")
        };
        if count(&candidate) <= limit {
            current = candidate;
        } else {
            if !current.is_empty() {
                pieces.push(current);
            }
            current = unit.to_owned();
        }
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
}

/// Split `text` into pieces of whole paragraphs, or whole lines when a
/// paragraph is too long, whose token counts stay under `limit`. Each piece
/// comes with the separator that joins it to the previous piece.
fn split(
    text: &str,
    limit: usize,
    count: impl Fn(&str) -> usize,
) -> Result<Vec<(&'static str, String)>> {
    let mut pieces = Vec::new();
    for paragraph in group(text.split("\n\n"), "\n\n", limit, &count) {
        if count(&paragraph) <= limit {
            pieces.push(("\n\n", paragraph));
            continue;
        }
        for (index, lines) in group(paragraph.split('\n'), "\n", limit, &count)
            .into_iter()
            .enumerate()
        {
            if count(&lines) > limit {
                bail!(InputError(format!(
                    "a line of the text is longer than {limit} tokens"
                )));
            }
            pieces.push((if index == 0 { "\n\n" } else { "\n" }, lines));
        }
    }
    Ok(pieces)
}

fn join(pieces: Vec<(&'static str, String)>) -> String {
    let mut joined = String::new();
    for (index, (separator, piece)) in pieces.into_iter().enumerate() {
        if index > 0 {
            joined.push_str(separator);
        }
        joined.push_str(&piece);
    }
    joined
}

pub struct Translator {
    backend: LlamaBackend,
    model: LlamaModel,
    stop: Vec<LlamaToken>,
    threads: i32,
    logs: bool,
}

impl Translator {
    pub fn load(path: &Path, logs: bool) -> Result<Self> {
        if logs {
            let _ = tracing_subscriber::fmt()
                .with_writer(std::io::stderr)
                .try_init();
        }
        send_logs_to_tracing(LogOptions::default().with_logs_enabled(logs));
        let backend = LlamaBackend::init().context("failed to start llama.cpp")?;
        let started = Instant::now();
        let model = LlamaModel::load_from_file(&backend, path, &LlamaModelParams::default())
            .with_context(|| format!("failed to load the model {}", path.display()))?;
        if logs {
            eprintln!("model: loaded in {} ms", started.elapsed().as_millis());
        }
        let stop = STOP_TOKENS
            .iter()
            .flat_map(|token| model.vocab().tokenize(token.as_bytes(), false, true))
            .collect();
        let threads = std::env::var("PLAMO_TR_LOCAL_THREADS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map_or(4, |n| (n.get() * 3 / 4).max(1))
                    .try_into()
                    .unwrap_or(4)
            });
        Ok(Translator {
            backend,
            model,
            stop,
            threads,
            logs,
        })
    }

    fn tokenize(&self, text: &str) -> Vec<LlamaToken> {
        self.model.vocab().tokenize(text.as_bytes(), true, true)
    }

    /// Translate `text`. A long text is translated a few paragraphs at a time.
    pub fn translate(&self, text: &str, options: &Options) -> Result<String> {
        check_input(text, options)?;
        let pieces = split(text, CHUNK_TOKENS, |piece| self.tokenize(piece).len())?;
        let mut translated = Vec::with_capacity(pieces.len());
        for (separator, piece) in pieces {
            translated.push((separator, self.generate(&prompt(&piece, options))?));
        }
        Ok(join(translated))
    }

    fn generate(&self, prompt: &str) -> Result<String> {
        let tokens = self.tokenize(prompt);
        let limit = self.model.n_ctx_train() as usize;
        let max_output = (tokens.len() * 3 + 256).min(limit.saturating_sub(tokens.len()));
        if max_output < 16 {
            bail!(InputError(format!(
                "the text does not fit in the {limit} tokens of the model"
            )));
        }
        let n_ctx = tokens.len() + max_output;
        let params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(n_ctx as u32))
            .with_n_batch(n_ctx as u32)
            .with_n_threads(self.threads)
            .with_n_threads_batch(self.threads);
        let mut context = self
            .model
            .new_context(&self.backend, params)
            .context("failed to create the llama.cpp context")?;
        let started = Instant::now();
        let mut batch = LlamaBatch::new(n_ctx, 1);
        batch.add_sequence(&tokens, 0, false)?;
        context
            .decode(&mut batch)
            .context("failed to read the prompt")?;
        let read = started.elapsed();
        let mut sampler = LlamaSampler::greedy();
        let vocab = self.model.vocab();
        let mut output = Vec::new();
        let mut generated = 0;
        let mut first = None;
        for position in (tokens.len() as i32..).take(max_output) {
            let token = sampler.sample(&context, batch.n_tokens() - 1);
            sampler.accept(token);
            first.get_or_insert_with(|| started.elapsed());
            if vocab.is_eog(token) || self.stop.contains(&token) {
                break;
            }
            generated += 1;
            output.extend(vocab.token_to_piece(token, false, None));
            batch.clear();
            batch.add(token, position, &[0], true)?;
            context.decode(&mut batch).context("failed to generate")?;
        }
        if self.logs {
            eprintln!(
                "prompt: {} tokens in {} ms, first token in {} ms, output: {generated} tokens in {} ms, threads: {}",
                tokens.len(),
                read.as_millis(),
                first.unwrap_or(read).as_millis(),
                (started.elapsed() - read).as_millis(),
                self.threads
            );
        }
        Ok(String::from_utf8_lossy(&output).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn direction(source: Option<&'static str>, target: &'static str) -> Direction {
        Direction { source, target }
    }

    #[test]
    fn prompt_follows_the_pfml3_template() {
        let direction = direction(Some("en"), "zh-Hans");
        let options = Options {
            direction: &direction,
            writing_style: None,
            context: None,
            glossary: &[],
        };
        assert_eq!(
            prompt("Hello", &options),
            "<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>input<|plamo:key|>lang<|plamo:val|>en<|plamo:msg|>Hello<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>zh-hans<|plamo:msg|>"
        );
    }

    #[test]
    fn prompt_carries_context_glossary_and_form() {
        let direction = direction(None, "ja");
        let glossary = [Term {
            source: "owner".into(),
            target: "担当者".into(),
        }];
        let options = Options {
            direction: &direction,
            writing_style: Some("polite"),
            context: Some("A task list"),
            glossary: &glossary,
        };
        assert_eq!(
            prompt("Filter by owner.", &options),
            "<|plamo:tag|>dataset<|plamo:msg|>translation<|plamo:tag|>context<|plamo:msg|>A task list<|plamo:tag|>glossary<|plamo:index|>owner<|plamo:item|>担当者<|plamo:tag|>input<|plamo:msg|>Filter by owner.<|plamo:tag|>output<|plamo:key|>lang<|plamo:val|>ja<|plamo:key|>form<|plamo:val|>polite<|plamo:msg|>"
        );
    }

    #[test]
    fn rejects_reserved_tokens_and_style_outside_japanese() {
        let to_ja = direction(None, "ja");
        let to_en = direction(None, "en");
        let options = |direction, writing_style| Options {
            direction,
            writing_style,
            context: None,
            glossary: &[],
        };
        assert!(check_input("a <|plamo:tag|> b", &options(&to_ja, None)).is_err());
        assert!(check_input("Hello", &options(&to_en, Some("plain"))).is_err());
        assert!(check_input("Hello", &options(&to_ja, Some("plain"))).is_ok());
    }

    #[test]
    fn splits_at_paragraphs_then_lines() {
        let count = |text: &str| text.len();
        let pieces = |text, limit| {
            split(text, limit, count)
                .unwrap()
                .into_iter()
                .map(|(_, piece)| piece)
                .collect::<Vec<_>>()
        };
        assert_eq!(pieces("aa\n\nbb", 10), vec!["aa\n\nbb"]);
        assert_eq!(pieces("aaaa\n\nbbbb", 5), vec!["aaaa", "bbbb"]);
        assert_eq!(pieces("aaa\nbbb\n\ncc", 5), vec!["aaa", "bbb", "cc"]);
        assert!(split("aaaaaa", 5, count).is_err());
    }

    #[test]
    fn joins_pieces_with_their_separators() {
        let text = "aaa\nbbb\n\ncc";
        assert_eq!(join(split(text, 5, |text: &str| text.len()).unwrap()), text);
    }
}
