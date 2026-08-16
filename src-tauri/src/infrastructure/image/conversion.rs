use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::Command;

use crate::core::image::ImageFormat;

#[derive(Debug, Clone)]
pub struct ImageConversionInfo {
    pub format: ImageFormat,
    pub virtual_size: u64,
    pub actual_size: u64,
    pub backing_file: Option<String>,
}

pub trait ImageConversionBackend: Send + Sync {
    fn info(&self, source: &Path) -> Result<ImageConversionInfo>;

    fn convert_to_raw(&self, source: &Path, destination: &Path) -> Result<()>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct QemuImgBackend;

impl QemuImgBackend {
    pub const fn new() -> Self {
        Self
    }

    fn execute(args: &[&str]) -> Result<String> {
        let output = Command::new(args[0])
            .args(&args[1..])
            .output()
            .with_context(|| format!("failed to execute '{}'", args[0]))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);

            bail!("{} failed: {}", args[0], stderr.trim());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    // fn parse_size(value: &str) -> Result<u64> {
    //     let value = value.trim();

    //     if let Ok(bytes) = value.parse::<u64>() {
    //         return Ok(bytes);
    //     }

    //     let mut number = String::new();

    //     let mut suffix = String::new();

    //     for character in value.chars() {
    //         if character.is_ascii_digit() || character == '.' {
    //             number.push(character);
    //         } else if !character.is_whitespace() {
    //             suffix.push(character);
    //         }
    //     }

    //     let number: f64 = number.parse()?;

    //     let multiplier = match suffix.to_ascii_lowercase().as_str() {
    //         "b" | "" => 1_f64,

    //         "k" | "kb" => 1024_f64,

    //         "m" | "mb" => 1024_f64.powi(2),

    //         "g" | "gb" => 1024_f64.powi(3),

    //         "t" | "tb" => 1024_f64.powi(4),

    //         "p" | "pb" => 1024_f64.powi(5),

    //         _ => {
    //             bail!("unsupported size '{}'", value)
    //         }
    //     };

    //     Ok((number * multiplier) as u64)
    // }

    fn parse_format(value: &str) -> Result<ImageFormat> {
        value.trim().parse::<ImageFormat>().map_err(|error| {
            anyhow::anyhow!("unsupported qemu image format '{}': {}", value, error)
        })
    }
}

impl ImageConversionBackend for QemuImgBackend {
    fn info(&self, source: &Path) -> Result<ImageConversionInfo> {
        let source_string = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("image path is not valid UTF-8"))?;

        let output = Self::execute(&["qemu-img", "info", "--output=json", source_string])?;

        let value: serde_json::Value =
            serde_json::from_str(&output).context("failed to parse qemu-img JSON output")?;

        let format = value
            .get("format")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("raw");

        let virtual_size = value
            .get("virtual-size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        let actual_size = value
            .get("actual-size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);

        let backing_file = value
            .get("backing-filename")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);

        Ok(ImageConversionInfo {
            format: Self::parse_format(format)?,

            virtual_size,

            actual_size,

            backing_file,
        })
    }

    fn convert_to_raw(&self, source: &Path, destination: &Path) -> Result<()> {
        let source_string = source
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("source path is not valid UTF-8"))?;

        let destination_string = destination
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("destination path is not valid UTF-8"))?;

        Self::execute(&[
            "qemu-img",
            "convert",
            "-p",
            "-f",
            "auto",
            "-O",
            "raw",
            source_string,
            destination_string,
        ])?;

        Ok(())
    }
}
