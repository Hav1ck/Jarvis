/*
Copyright (C) 2025  Hav1ck

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU Affero General Public License as
published by the Free Software Foundation, either version 3 of the
License, or (at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU Affero General Public License for more details.

You should have received a copy of the GNU Affero General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use crate::models::Config;
use anyhow::{Context, Result, anyhow};
use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client as ReqwestClient;
use std::cmp::min;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

// loads the config from the specified JSON file
pub fn load_config(path: &Path) -> Result<Config> {
    println!("[DEBUG] Entered load_config with path: {}", path.display());
    let file = File::open(path)
        .with_context(|| format!("failed to open config file `{}`", path.display()))?;

    let config = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse JSON from `{}`", path.display()))?;
    println!("[DEBUG] Loaded config from file");
    Ok(config)
}

// downloads a file from the given URL and saves it to the specified path
pub async fn download_file(url: &str, path: &Path) -> Result<()> {
    println!(
        "[DEBUG] Entered download_file with url: {} to path: {}",
        url,
        path.display()
    );
    if path.exists() {
        println!(
            "[DEBUG] File already exists at {}. Skipping download.",
            path.display()
        );
        return Ok(());
    }
    ensure_parent_directory_exists(path).context("while ensuring download directory exists")?;

    let client = ReqwestClient::new();
    let res = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to GET from {}", url))?;

    let total_size = res
        .content_length()
        .ok_or_else(|| anyhow!("failed to get content-length from {}", url))?;

    let pb = ProgressBar::new(total_size);

    let style = ProgressStyle::with_template("{msg}\n{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .with_context(|| "failed to parse progress‐bar template")?
        .progress_chars("#>-");

    pb.set_style(style);

    let file_name = path
        .file_name()
        .ok_or_else(|| anyhow!("failed to find file name in path {}", path.display()))?
        .to_string_lossy();

    pb.set_message(format!("Downloading {}", file_name));

    let mut file =
        File::create(path).with_context(|| format!("failed to create file {}", path.display()))?;

    let mut downloaded: u64 = 0;
    let mut stream = res.bytes_stream();

    while let Some(item) = stream.next().await {
        let chunk = item.with_context(|| format!("error while downloading chunk from {}", url))?;

        file.write_all(&chunk)
            .with_context(|| format!("failed to write to file {}", path.display()))?;

        downloaded = min(downloaded + chunk.len() as u64, total_size);
        pb.set_position(downloaded);
    }

    pb.finish_with_message(format!("Download of {} complete.", file_name));
    println!("[DEBUG] Finished downloading file");
    Ok(())
}

// converts i16 audio samples to f32, required for whisper tts
pub fn convert_i16_to_f32(samples: &[i16]) -> Vec<f32> {
    println!("[DEBUG] Converting i16 samples to f32");
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

// ensures that the directory for the given path exists, creating it if necessary
pub fn ensure_parent_directory_exists(path: &Path) -> Result<()> {
    println!(
        "[DEBUG] Ensuring parent directory exists for path: {}",
        path.display()
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    Ok(())
}
