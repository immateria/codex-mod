use serde_json::Value;
use std::io::IsTerminal;
use std::io::Read;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromptDecodeError {
    InvalidUtf8 { valid_up_to: usize },
    InvalidUtf16 { encoding: &'static str },
    UnsupportedBom { encoding: &'static str },
}

impl std::fmt::Display for PromptDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptDecodeError::InvalidUtf8 { valid_up_to } => write!(
                f,
                "input is not valid UTF-8 (invalid byte at offset {valid_up_to}). Convert it to UTF-8 and retry (e.g., `iconv -f <ENC> -t UTF-8 prompt.txt`)."
            ),
            PromptDecodeError::InvalidUtf16 { encoding } => write!(
                f,
                "input looked like {encoding} but could not be decoded. Convert it to UTF-8 and retry."
            ),
            PromptDecodeError::UnsupportedBom { encoding } => write!(
                f,
                "input appears to be {encoding}. Convert it to UTF-8 and retry."
            ),
        }
    }
}

pub(crate) fn decode_prompt_bytes(input: &[u8]) -> Result<String, PromptDecodeError> {
    let input = input.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(input);

    if input.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32LE",
        });
    }

    if input.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return Err(PromptDecodeError::UnsupportedBom {
            encoding: "UTF-32BE",
        });
    }

    if let Some(rest) = input.strip_prefix(&[0xFF, 0xFE]) {
        return decode_utf16(rest, "UTF-16LE", u16::from_le_bytes);
    }

    if let Some(rest) = input.strip_prefix(&[0xFE, 0xFF]) {
        return decode_utf16(rest, "UTF-16BE", u16::from_be_bytes);
    }

    std::str::from_utf8(input)
        .map(str::to_string)
        .map_err(|error| PromptDecodeError::InvalidUtf8 {
            valid_up_to: error.valid_up_to(),
        })
}

fn decode_utf16(
    input: &[u8],
    encoding: &'static str,
    decode_unit: fn([u8; 2]) -> u16,
) -> Result<String, PromptDecodeError> {
    if !input.len().is_multiple_of(2) {
        return Err(PromptDecodeError::InvalidUtf16 { encoding });
    }

    let units: Vec<u16> = input
        .chunks_exact(2)
        .map(|chunk| decode_unit([chunk[0], chunk[1]]))
        .collect();

    String::from_utf16(&units).map_err(|_| PromptDecodeError::InvalidUtf16 { encoding })
}

pub(crate) fn resolve_prompt(prompt_arg: Option<String>) -> String {
    match prompt_arg {
        Some(prompt) if prompt != "-" => prompt,
        maybe_dash => {
            let force_stdin = matches!(maybe_dash.as_deref(), Some("-"));

            if std::io::stdin().is_terminal() && !force_stdin {
                eprintln!(
                    "No prompt provided. Either specify one as an argument or pipe the prompt into stdin."
                );
                std::process::exit(1);
            }

            if !force_stdin {
                eprintln!("Reading prompt from stdin...");
            }

            let mut bytes = Vec::new();
            if let Err(error) = std::io::stdin().read_to_end(&mut bytes) {
                eprintln!("Failed to read prompt from stdin: {error}");
                std::process::exit(1);
            }

            let prompt = match decode_prompt_bytes(&bytes) {
                Ok(decoded) => decoded,
                Err(error) => {
                    eprintln!("Failed to read prompt from stdin: {error}");
                    std::process::exit(1);
                }
            };

            if prompt.trim().is_empty() {
                eprintln!("No prompt provided via stdin.");
                std::process::exit(1);
            }

            prompt
        }
    }
}

pub(crate) fn load_output_schema(path: Option<PathBuf>) -> Option<Value> {
    let path = path?;

    let schema_str = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(err) => {
            eprintln!(
                "Failed to read output schema file {}: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    };

    match serde_json::from_str::<Value>(&schema_str) {
        Ok(value) => Some(value),
        Err(err) => {
            eprintln!(
                "Output schema file {} is not valid JSON: {err}",
                path.display()
            );
            std::process::exit(1);
        }
    }
}
