use std::fs::File;
use std::io::{BufWriter, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelSize {
    Size0_8b,
    Size2b,
    Size8b,
    Size31b,
}

impl ModelSize {
    pub const ALL: [ModelSize; 4] = [
        ModelSize::Size0_8b,
        ModelSize::Size2b,
        ModelSize::Size8b,
        ModelSize::Size31b,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ModelSize::Size0_8b => "0.8b",
            ModelSize::Size2b => "2b",
            ModelSize::Size8b => "8b",
            ModelSize::Size31b => "31b",
        }
    }

    pub fn repository(self) -> String {
        format!("pfnet/plamo-3-translate-{}-GGUF", self.as_str())
    }

    pub fn file_name(self) -> String {
        format!("plamo-3-translate-{}-Q4_K_M.gguf", self.as_str())
    }

    pub fn url(self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/main/{}",
            self.repository(),
            self.file_name()
        )
    }
}

pub fn cache_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PLAMO_TR_LOCAL_CACHE_DIR") {
        return Ok(path.into());
    }
    let dir = dirs::cache_dir().context("could not determine the user cache directory")?;
    Ok(dir.join("plamo-tr-local"))
}

pub fn cached_path(size: ModelSize) -> Result<PathBuf> {
    Ok(cache_dir()?.join(size.file_name()))
}

/// The path of the model file, downloaded into the cache on first use.
pub fn ensure(size: ModelSize) -> Result<PathBuf> {
    let path = cached_path(size)?;
    if !path.exists() {
        download(size, &path)?;
    }
    Ok(path)
}

pub fn download(size: ModelSize, path: &Path) -> Result<()> {
    let dir = path
        .parent()
        .context("the model path has no parent directory")?;
    std::fs::create_dir_all(dir)
        .with_context(|| format!("failed to create the cache directory {}", dir.display()))?;
    let url = size.url();
    eprintln!("downloading {url}");
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .context("failed to build the HTTP client")?;
    let mut response = client
        .get(&url)
        .send()
        .with_context(|| format!("failed to download {url}"))?;
    if !response.status().is_success() {
        bail!("failed to download {url}: {}", response.status());
    }
    let total = response.content_length();
    let partial = path.with_extension("gguf.part");
    let mut file = BufWriter::new(
        File::create(&partial)
            .with_context(|| format!("failed to create {}", partial.display()))?,
    );
    let progress = std::io::stderr().is_terminal();
    let mut buffer = vec![0; 1 << 20];
    let mut written: u64 = 0;
    loop {
        let read = response
            .read(&mut buffer)
            .with_context(|| format!("failed to download {url}"))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .with_context(|| format!("failed to write {}", partial.display()))?;
        written += read as u64;
        if progress {
            match total {
                Some(total) => eprint!("\r{} / {} MB", written / 1_000_000, total / 1_000_000),
                None => eprint!("\r{} MB", written / 1_000_000),
            }
        }
    }
    if progress {
        eprintln!();
    }
    file.flush()
        .with_context(|| format!("failed to write {}", partial.display()))?;
    if let Some(total) = total
        && written != total
    {
        bail!("download of {url} ended after {written} of {total} bytes");
    }
    std::fs::rename(&partial, path)
        .with_context(|| format!("failed to move the model to {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_point_at_the_q4_k_m_gguf() {
        assert_eq!(
            ModelSize::Size2b.url(),
            "https://huggingface.co/pfnet/plamo-3-translate-2b-GGUF/resolve/main/plamo-3-translate-2b-Q4_K_M.gguf"
        );
        assert_eq!(
            ModelSize::Size0_8b.file_name(),
            "plamo-3-translate-0.8b-Q4_K_M.gguf"
        );
    }
}
